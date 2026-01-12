use crate::event_processor::utils::watcher::WatcherTest;
use anyhow::Result;
use nexus_common::db::RedisOps;
use nexus_common::models::user::UserCounts;
use pubky::Keypair;
use pubky_app_specs::PubkyAppUser;

/// Tests that concurrent user creation doesn't cause race conditions in UserCounts initialization.
/// This test verifies the TOCTOU fix where `put_to_index_nx` is used instead of check-then-create.
#[tokio_shared_rt::test(shared)]
async fn test_user_counts_atomic_initialization() -> Result<()> {
    let _test = WatcherTest::setup().await?;

    let user_kp = Keypair::random();
    let user_id = user_kp.public_key().to_z32();

    // First, ensure no counts exist for this user
    let initial_counts = UserCounts::get_from_index(&user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(initial_counts.is_none(), "User counts should not exist yet");

    // Test that put_to_index_nx creates counts on first call
    let was_created = UserCounts::default()
        .put_to_index_nx(&user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(was_created, "First put_to_index_nx should create the counts");

    // Verify counts were created
    let counts_after_first = UserCounts::get_from_index(&user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(
        counts_after_first.is_some(),
        "Counts should exist after first put_to_index_nx"
    );

    // Test that put_to_index_nx returns false on second call (already exists)
    let was_created_again = UserCounts::default()
        .put_to_index_nx(&user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(
        !was_created_again,
        "Second put_to_index_nx should return false (already exists)"
    );

    // Verify counts weren't overwritten (values should still be defaults)
    let counts_after_second = UserCounts::get_from_index(&user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .expect("Counts should still exist");
    assert_eq!(counts_after_second.followers, 0);
    assert_eq!(counts_after_second.following, 0);
    assert_eq!(counts_after_second.posts, 0);

    // Clean up
    UserCounts::remove_from_index_multiple_json(&[&[&user_id]])
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

/// Tests concurrent put_to_index_nx calls don't cause data corruption.
/// Simulates the race condition scenario that the TOCTOU fix addresses.
#[tokio_shared_rt::test(shared)]
async fn test_user_counts_concurrent_initialization() -> Result<()> {
    let _test = WatcherTest::setup().await?;

    let user_kp = Keypair::random();
    let user_id = user_kp.public_key().to_z32();

    // Ensure no counts exist
    let initial_counts = UserCounts::get_from_index(&user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(initial_counts.is_none(), "User counts should not exist yet");

    // Simulate concurrent initialization attempts
    let user_id_clone = user_id.clone();
    let handle1 = tokio::spawn(async move {
        UserCounts::default()
            .put_to_index_nx(&user_id_clone)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    });

    let user_id_clone2 = user_id.clone();
    let handle2 = tokio::spawn(async move {
        UserCounts::default()
            .put_to_index_nx(&user_id_clone2)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    });

    let user_id_clone3 = user_id.clone();
    let handle3 = tokio::spawn(async move {
        UserCounts::default()
            .put_to_index_nx(&user_id_clone3)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    });

    // Wait for all concurrent operations
    let (result1, result2, result3) = tokio::join!(handle1, handle2, handle3);

    let was_created1 = result1??;
    let was_created2 = result2??;
    let was_created3 = result3??;

    // Exactly one should have created the record
    let created_count = [was_created1, was_created2, was_created3]
        .iter()
        .filter(|&&x| x)
        .count();
    assert_eq!(
        created_count, 1,
        "Exactly one concurrent put_to_index_nx should create the record, but {} did",
        created_count
    );

    // Verify counts exist and have correct default values
    let final_counts = UserCounts::get_from_index(&user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .expect("Counts should exist after concurrent initialization");
    assert_eq!(final_counts.followers, 0);
    assert_eq!(final_counts.following, 0);
    assert_eq!(final_counts.posts, 0);

    // Clean up
    UserCounts::remove_from_index_multiple_json(&[&[&user_id]])
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

/// Tests the full user creation flow with the TOCTOU fix.
#[tokio_shared_rt::test(shared)]
async fn test_user_creation_with_atomic_counts() -> Result<()> {
    let mut test = WatcherTest::setup().await?;

    let user_kp = Keypair::random();
    let user = PubkyAppUser {
        bio: Some("test_atomic_user_counts".to_string()),
        image: None,
        links: None,
        name: "AtomicCountsUser".to_string(),
        status: None,
    };

    let user_id = test.create_user(&user_kp, &user).await?;

    // Verify user counts were created
    let user_counts = UserCounts::get_from_index(&user_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .expect("User counts should be created during user creation");

    assert_eq!(user_counts.followers, 0);
    assert_eq!(user_counts.following, 0);
    assert_eq!(user_counts.posts, 0);

    // Cleanup
    test.cleanup_user(&user_kp).await?;

    Ok(())
}

use crate::event_processor::utils::watcher::WatcherTest;
use anyhow::Result;
use nexus_common::db::RedisOps;
use nexus_common::models::post::PostCounts;
use pubky::Keypair;
use pubky_app_specs::{PubkyAppPost, PubkyAppPostKind, PubkyAppUser};

/// Tests that concurrent post creation doesn't cause race conditions in PostCounts initialization.
/// This test verifies the TOCTOU fix where `put_to_index_nx` is used instead of check-then-create.
#[tokio_shared_rt::test(shared)]
async fn test_post_counts_atomic_initialization() -> Result<()> {
    let _test = WatcherTest::setup().await?;

    let user_kp = Keypair::random();
    let user_id = user_kp.public_key().to_z32();
    let post_id = "test_post_atomic_init";

    // First, ensure no counts exist for this post
    let initial_counts = PostCounts::get_from_index(&user_id, post_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(initial_counts.is_none(), "Post counts should not exist yet");

    // Test that put_to_index_nx creates counts on first call (is_reply = false)
    let was_created = PostCounts::default()
        .put_to_index_nx(&user_id, post_id, false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(was_created, "First put_to_index_nx should create the counts");

    // Verify counts were created
    let counts_after_first = PostCounts::get_from_index(&user_id, post_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(
        counts_after_first.is_some(),
        "Counts should exist after first put_to_index_nx"
    );

    // Test that put_to_index_nx returns false on second call (already exists)
    let was_created_again = PostCounts::default()
        .put_to_index_nx(&user_id, post_id, false)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(
        !was_created_again,
        "Second put_to_index_nx should return false (already exists)"
    );

    // Verify counts weren't overwritten (values should still be defaults)
    let counts_after_second = PostCounts::get_from_index(&user_id, post_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .expect("Counts should still exist");
    assert_eq!(counts_after_second.replies, 0);
    assert_eq!(counts_after_second.reposts, 0);
    assert_eq!(counts_after_second.tags, 0);

    // Clean up
    PostCounts::remove_from_index_multiple_json(&[&[&user_id, post_id]])
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

/// Tests concurrent put_to_index_nx calls for posts don't cause data corruption.
/// Simulates the race condition scenario that the TOCTOU fix addresses.
#[tokio_shared_rt::test(shared)]
async fn test_post_counts_concurrent_initialization() -> Result<()> {
    let _test = WatcherTest::setup().await?;

    let user_kp = Keypair::random();
    let user_id = user_kp.public_key().to_z32();
    let post_id = "test_post_concurrent";

    // Ensure no counts exist
    let initial_counts = PostCounts::get_from_index(&user_id, post_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(initial_counts.is_none(), "Post counts should not exist yet");

    // Simulate concurrent initialization attempts
    let user_id_clone = user_id.clone();
    let post_id_clone = post_id.to_string();
    let handle1 = tokio::spawn(async move {
        PostCounts::default()
            .put_to_index_nx(&user_id_clone, &post_id_clone, false)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    });

    let user_id_clone2 = user_id.clone();
    let post_id_clone2 = post_id.to_string();
    let handle2 = tokio::spawn(async move {
        PostCounts::default()
            .put_to_index_nx(&user_id_clone2, &post_id_clone2, false)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    });

    let user_id_clone3 = user_id.clone();
    let post_id_clone3 = post_id.to_string();
    let handle3 = tokio::spawn(async move {
        PostCounts::default()
            .put_to_index_nx(&user_id_clone3, &post_id_clone3, false)
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
    let final_counts = PostCounts::get_from_index(&user_id, post_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .expect("Counts should exist after concurrent initialization");
    assert_eq!(final_counts.replies, 0);
    assert_eq!(final_counts.reposts, 0);
    assert_eq!(final_counts.tags, 0);

    // Clean up
    PostCounts::remove_from_index_multiple_json(&[&[&user_id, post_id]])
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

/// Tests post counts initialization for replies (is_reply = true).
/// Replies should not be added to global engagement sorted sets.
#[tokio_shared_rt::test(shared)]
async fn test_post_counts_reply_initialization() -> Result<()> {
    let _test = WatcherTest::setup().await?;

    let user_kp = Keypair::random();
    let user_id = user_kp.public_key().to_z32();
    let post_id = "test_reply_post";

    // Test that put_to_index_nx creates counts for a reply
    let was_created = PostCounts::default()
        .put_to_index_nx(&user_id, post_id, true) // is_reply = true
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    assert!(
        was_created,
        "First put_to_index_nx should create the counts for reply"
    );

    // Verify counts were created
    let counts = PostCounts::get_from_index(&user_id, post_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .expect("Counts should exist for reply");
    assert_eq!(counts.replies, 0);
    assert_eq!(counts.reposts, 0);

    // Clean up
    PostCounts::remove_from_index_multiple_json(&[&[&user_id, post_id]])
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

/// Tests the full post creation flow with the TOCTOU fix.
#[tokio_shared_rt::test(shared)]
async fn test_post_creation_with_atomic_counts() -> Result<()> {
    let mut test = WatcherTest::setup().await?;

    let user_kp = Keypair::random();
    let user = PubkyAppUser {
        bio: Some("test_atomic_post_counts".to_string()),
        image: None,
        links: None,
        name: "AtomicPostCountsUser".to_string(),
        status: None,
    };

    let user_id = test.create_user(&user_kp, &user).await?;

    let post = PubkyAppPost {
        content: "Test post for atomic counts".to_string(),
        kind: PubkyAppPostKind::Short,
        parent: None,
        embed: None,
        attachments: None,
    };

    let (post_id, post_path) = test.create_post(&user_kp, &post).await?;

    // Verify post counts were created
    let post_counts = PostCounts::get_from_index(&user_id, &post_id)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .expect("Post counts should be created during post creation");

    assert_eq!(post_counts.replies, 0);
    assert_eq!(post_counts.reposts, 0);
    assert_eq!(post_counts.tags, 0);

    // Cleanup
    test.cleanup_post(&user_kp, &post_path).await?;
    test.cleanup_user(&user_kp).await?;

    Ok(())
}

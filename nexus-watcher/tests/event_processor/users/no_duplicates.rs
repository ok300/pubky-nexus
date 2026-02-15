use crate::event_processor::utils::watcher::WatcherTest;
use anyhow::Result;
use nexus_common::{
    db::RedisOps,
    models::user::{UserSearch, USER_ID_KEY_PARTS, USER_NAME_KEY_PARTS},
};
use pubky::Keypair;
use pubky_app_specs::PubkyAppUser;

/// Test that changing username doesn't leave duplicate entries in sorted sets
#[tokio_shared_rt::test(shared)]
async fn test_no_duplicate_after_username_change() -> Result<()> {
    let mut test = WatcherTest::setup().await?;

    let keypair = Keypair::random();
    let original_name = "OriginalUsername";
    let user = PubkyAppUser {
        bio: Some("Testing username change".to_string()),
        image: None,
        links: None,
        name: original_name.to_string(),
        status: None,
    };

    let user_id = test.create_user(&keypair, &user).await?;

    // Verify original user is in sorted sets
    let is_member = UserSearch::check_sorted_set_member(
        None,
        &USER_NAME_KEY_PARTS,
        &[&original_name.to_lowercase(), &user_id],
    )
    .await
    .unwrap();
    assert!(is_member.is_some(), "User should be in name sorted set");

    let is_member_id = UserSearch::check_sorted_set_member(
        None,
        &USER_ID_KEY_PARTS,
        &[&user_id],
    )
    .await
    .unwrap();
    assert!(is_member_id.is_some(), "User should be in ID sorted set");

    // Change username using create_profile which updates existing user
    let new_name = "NewUsername";
    let updated_user = PubkyAppUser {
        bio: Some("Testing username change".to_string()),
        image: None,
        links: None,
        name: new_name.to_string(),
        status: None,
    };
    test.create_profile(&user_id, &updated_user).await?;

    // Verify new username is in sorted set
    let is_member_new = UserSearch::check_sorted_set_member(
        None,
        &USER_NAME_KEY_PARTS,
        &[&new_name.to_lowercase(), &user_id],
    )
    .await
    .unwrap();
    assert!(
        is_member_new.is_some(),
        "User with new username should be in sorted set"
    );

    // Verify old username is NOT in sorted set (no duplicate)
    let is_member_old = UserSearch::check_sorted_set_member(
        None,
        &USER_NAME_KEY_PARTS,
        &[&original_name.to_lowercase(), &user_id],
    )
    .await
    .unwrap();
    assert!(
        is_member_old.is_none(),
        "User with old username should NOT be in sorted set"
    );

    // Verify user_id is still in ID sorted set (should be only once)
    let is_member_id = UserSearch::check_sorted_set_member(
        None,
        &USER_ID_KEY_PARTS,
        &[&user_id],
    )
    .await
    .unwrap();
    assert!(is_member_id.is_some(), "User should still be in ID sorted set");

    // Cleanup
    test.cleanup_user(&user_id).await?;

    Ok(())
}

/// Test that deleting a user removes all entries from sorted sets
#[tokio_shared_rt::test(shared)]
async fn test_no_orphaned_entries_after_delete() -> Result<()> {
    let mut test = WatcherTest::setup().await?;

    let keypair = Keypair::random();
    let username = "UserToDelete";
    let user = PubkyAppUser {
        bio: Some("Testing deletion cleanup".to_string()),
        image: None,
        links: None,
        name: username.to_string(),
        status: None,
    };

    let user_id = test.create_user(&keypair, &user).await?;

    // Verify user is in sorted sets before deletion
    let is_member = UserSearch::check_sorted_set_member(
        None,
        &USER_NAME_KEY_PARTS,
        &[&username.to_lowercase(), &user_id],
    )
    .await
    .unwrap();
    assert!(is_member.is_some(), "User should be in name sorted set");

    let is_member_id = UserSearch::check_sorted_set_member(
        None,
        &USER_ID_KEY_PARTS,
        &[&user_id],
    )
    .await
    .unwrap();
    assert!(is_member_id.is_some(), "User should be in ID sorted set");

    // Delete the user
    test.cleanup_user(&user_id).await?;

    // Verify user is NOT in sorted sets after deletion (no orphaned entries)
    let is_member_after = UserSearch::check_sorted_set_member(
        None,
        &USER_NAME_KEY_PARTS,
        &[&username.to_lowercase(), &user_id],
    )
    .await
    .unwrap();
    assert!(
        is_member_after.is_none(),
        "User should NOT be in name sorted set after deletion"
    );

    let is_member_id_after = UserSearch::check_sorted_set_member(
        None,
        &USER_ID_KEY_PARTS,
        &[&user_id],
    )
    .await
    .unwrap();
    assert!(
        is_member_id_after.is_none(),
        "User should NOT be in ID sorted set after deletion"
    );

    Ok(())
}

/// Test that multiple username changes don't create duplicates
#[tokio_shared_rt::test(shared)]
async fn test_multiple_username_changes_no_duplicates() -> Result<()> {
    let mut test = WatcherTest::setup().await?;

    let keypair = Keypair::random();
    let names = vec!["FirstName", "SecondName", "ThirdName", "FourthName"];
    
    // Create user with first name
    let user = PubkyAppUser {
        bio: Some("Testing multiple username changes".to_string()),
        image: None,
        links: None,
        name: names[0].to_string(),
        status: None,
    };
    let user_id = test.create_user(&keypair, &user).await?;

    // Change username multiple times using create_profile
    for name in &names[1..] {
        let updated_user = PubkyAppUser {
            bio: Some("Testing multiple username changes".to_string()),
            image: None,
            links: None,
            name: name.to_string(),
            status: None,
        };
        test.create_profile(&user_id, &updated_user).await?;
    }

    // Verify only the last username exists in the sorted set
    let is_member_last = UserSearch::check_sorted_set_member(
        None,
        &USER_NAME_KEY_PARTS,
        &[&names[names.len() - 1].to_lowercase(), &user_id],
    )
    .await
    .unwrap();
    assert!(
        is_member_last.is_some(),
        "User with final username should be in sorted set"
    );

    // Verify all previous usernames are NOT in sorted set
    for name in &names[..names.len() - 1] {
        let is_member_old = UserSearch::check_sorted_set_member(
            None,
            &USER_NAME_KEY_PARTS,
            &[&name.to_lowercase(), &user_id],
        )
        .await
        .unwrap();
        assert!(
            is_member_old.is_none(),
            "Old username '{}' should NOT be in sorted set",
            name
        );
    }

    // Verify user_id exists only once in ID sorted set
    let is_member_id = UserSearch::check_sorted_set_member(
        None,
        &USER_ID_KEY_PARTS,
        &[&user_id],
    )
    .await
    .unwrap();
    assert!(
        is_member_id.is_some(),
        "User ID should be in sorted set exactly once"
    );

    // Cleanup
    test.cleanup_user(&user_id).await?;

    Ok(())
}

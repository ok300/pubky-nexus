use crate::event_processor::utils::watcher::{HomeserverHashIdPath, WatcherTest};
use anyhow::Result;
use chrono::Utc;
use nexus_common::models::tag::search::TagSearch;
use nexus_common::types::Pagination;
use pubky::Keypair;
use pubky_app_specs::post_uri_builder;
use pubky_app_specs::{PubkyAppPost, PubkyAppTag, PubkyAppUser};

/// Test that verifies unused tags are removed from autosuggestions
/// when the last post with that tag is removed
#[tokio_shared_rt::test(shared)]
async fn test_tag_search_cleanup_after_post_del() -> Result<()> {
    let mut test = WatcherTest::setup().await?;

    // Step 1: Create a user
    let author_kp = Keypair::random();
    let author = PubkyAppUser {
        bio: Some("test_tag_search_cleanup".to_string()),
        image: None,
        links: None,
        name: "TagSearchCleanup:User".to_string(),
        status: None,
    };
    let author_user_id = test.create_user(&author_kp, &author).await?;

    // Step 2: Create a post
    let post = PubkyAppPost {
        content: "TagSearchCleanup:Post".to_string(),
        kind: PubkyAppPost::default().kind,
        parent: None,
        embed: None,
        attachments: None,
    };
    let (post_id, post_path) = test.create_post(&author_kp, &post).await?;

    // Step 3: Add a unique tag to the post
    let unique_label = format!("uniquetag{}", Utc::now().timestamp_millis());

    let tag = PubkyAppTag {
        uri: post_uri_builder(author_user_id.clone(), post_id.clone()),
        label: unique_label.clone(),
        created_at: Utc::now().timestamp_millis(),
    };
    let tag_path = tag.hs_path();
    test.put(&author_kp, &tag_path, tag).await?;

    // Step 4: Verify the tag appears in search suggestions
    let search_results = match TagSearch::get_by_label(&unique_label, &Pagination::default()).await {
        Ok(Some(results)) => results,
        Ok(None) => vec![],
        Err(e) => panic!("Failed to search tags: {}", e),
    };

    assert!(
        !search_results.is_empty(),
        "Tag should appear in search suggestions after being added"
    );

    // Step 5: Delete the tag
    test.del(&author_kp, &tag_path).await?;

    // Step 6: Verify the tag no longer appears in search suggestions
    let search_results_after_del = match TagSearch::get_by_label(&unique_label, &Pagination::default()).await {
        Ok(Some(results)) => results,
        Ok(None) => vec![],
        Err(e) => panic!("Failed to search tags after deletion: {}", e),
    };

    assert!(
        search_results_after_del.is_empty(),
        "Tag should be removed from search suggestions after last occurrence is deleted"
    );

    // Cleanup
    test.cleanup_post(&author_kp, &post_path).await?;
    test.cleanup_user(&author_kp).await?;

    Ok(())
}

/// Test that verifies unused tags are removed from autosuggestions
/// when the last user with that tag is untagged
#[tokio_shared_rt::test(shared)]
async fn test_tag_search_cleanup_after_user_del() -> Result<()> {
    let mut test = WatcherTest::setup().await?;

    // Step 1: Create a tagger user
    let tagger_kp = Keypair::random();
    let tagger = PubkyAppUser {
        bio: Some("test_tag_search_cleanup_user".to_string()),
        image: None,
        links: None,
        name: "TagSearchCleanup:Tagger".to_string(),
        status: None,
    };
    let tagger_user_id = test.create_user(&tagger_kp, &tagger).await?;

    // Step 2: Create a tagged user
    let tagged_kp = Keypair::random();
    let tagged = PubkyAppUser {
        bio: Some("test_tag_search_cleanup_tagged".to_string()),
        image: None,
        links: None,
        name: "TagSearchCleanup:Tagged".to_string(),
        status: None,
    };
    let tagged_user_id = test.create_user(&tagged_kp, &tagged).await?;

    // Step 3: Add a unique tag to the user
    let unique_label = format!("uniqueusertag{}", Utc::now().timestamp_millis());

    let tag = PubkyAppTag {
        uri: format!("pubky://{}", tagged_user_id),
        label: unique_label.clone(),
        created_at: Utc::now().timestamp_millis(),
    };
    let tag_path = tag.hs_path();
    test.put(&tagger_kp, &tag_path, tag).await?;

    // Step 4: Verify the tag appears in search suggestions
    let search_results = match TagSearch::get_by_label(&unique_label, &Pagination::default()).await {
        Ok(Some(results)) => results,
        Ok(None) => vec![],
        Err(e) => panic!("Failed to search tags: {}", e),
    };

    assert!(
        !search_results.is_empty(),
        "Tag should appear in search suggestions after being added to user"
    );

    // Step 5: Delete the tag
    test.del(&tagger_kp, &tag_path).await?;

    // Step 6: Verify the tag no longer appears in search suggestions
    let search_results_after_del = match TagSearch::get_by_label(&unique_label, &Pagination::default()).await {
        Ok(Some(results)) => results,
        Ok(None) => vec![],
        Err(e) => panic!("Failed to search tags after deletion: {}", e),
    };

    assert!(
        search_results_after_del.is_empty(),
        "Tag should be removed from search suggestions after last user tag is deleted"
    );

    // Cleanup
    test.cleanup_user(&tagger_kp).await?;
    test.cleanup_user(&tagged_kp).await?;

    Ok(())
}

/// Test that verifies a tag is NOT removed from autosuggestions
/// if it still has other occurrences (e.g., on another post)
#[tokio_shared_rt::test(shared)]
async fn test_tag_search_preserved_with_multiple_posts() -> Result<()> {
    let mut test = WatcherTest::setup().await?;

    // Step 1: Create a user
    let author_kp = Keypair::random();
    let author = PubkyAppUser {
        bio: Some("test_tag_preserved".to_string()),
        image: None,
        links: None,
        name: "TagPreserved:User".to_string(),
        status: None,
    };
    let author_user_id = test.create_user(&author_kp, &author).await?;

    // Step 2: Create first post
    let post1 = PubkyAppPost {
        content: "TagPreserved:Post1".to_string(),
        kind: PubkyAppPost::default().kind,
        parent: None,
        embed: None,
        attachments: None,
    };
    let (post1_id, post1_path) = test.create_post(&author_kp, &post1).await?;

    // Step 3: Create second post
    let post2 = PubkyAppPost {
        content: "TagPreserved:Post2".to_string(),
        kind: PubkyAppPost::default().kind,
        parent: None,
        embed: None,
        attachments: None,
    };
    let (post2_id, post2_path) = test.create_post(&author_kp, &post2).await?;

    // Step 4: Add the same tag to both posts
    let shared_label = format!("sharedtag{}", Utc::now().timestamp_millis());

    let tag1 = PubkyAppTag {
        uri: post_uri_builder(author_user_id.clone(), post1_id.clone()),
        label: shared_label.clone(),
        created_at: Utc::now().timestamp_millis(),
    };
    let tag1_path = tag1.hs_path();
    test.put(&author_kp, &tag1_path, tag1).await?;

    let tag2 = PubkyAppTag {
        uri: post_uri_builder(author_user_id.clone(), post2_id.clone()),
        label: shared_label.clone(),
        created_at: Utc::now().timestamp_millis(),
    };
    let tag2_path = tag2.hs_path();
    test.put(&author_kp, &tag2_path, tag2).await?;

    // Step 5: Verify the tag appears in search suggestions
    let search_results = match TagSearch::get_by_label(&shared_label, &Pagination::default()).await {
        Ok(Some(results)) => results,
        Ok(None) => vec![],
        Err(e) => panic!("Failed to search tags: {}", e),
    };

    assert!(
        !search_results.is_empty(),
        "Tag should appear in search suggestions"
    );

    // Step 6: Delete tag from first post only
    test.del(&author_kp, &tag1_path).await?;

    // Step 7: Verify the tag STILL appears in search suggestions
    // because it's still on the second post
    let search_results_after_del = match TagSearch::get_by_label(&shared_label, &Pagination::default()).await {
        Ok(Some(results)) => results,
        Ok(None) => vec![],
        Err(e) => panic!("Failed to search tags after first deletion: {}", e),
    };

    assert!(
        !search_results_after_del.is_empty(),
        "Tag should still appear in search suggestions because it exists on another post"
    );

    // Step 8: Delete tag from second post
    test.del(&author_kp, &tag2_path).await?;

    // Step 9: Now the tag should be removed from search suggestions
    let search_results_final = match TagSearch::get_by_label(&shared_label, &Pagination::default()).await {
        Ok(Some(results)) => results,
        Ok(None) => vec![],
        Err(e) => panic!("Failed to search tags after final deletion: {}", e),
    };

    assert!(
        search_results_final.is_empty(),
        "Tag should be removed from search suggestions after all occurrences are deleted"
    );

    // Cleanup
    test.cleanup_post(&author_kp, &post1_path).await?;
    test.cleanup_post(&author_kp, &post2_path).await?;
    test.cleanup_user(&author_kp).await?;

    Ok(())
}

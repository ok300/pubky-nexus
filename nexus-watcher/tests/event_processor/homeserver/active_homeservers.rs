use crate::event_processor::utils::watcher::WatcherTest;
use anyhow::Result;
use nexus_common::db::exec_single_row;
use nexus_common::db::queries;
use nexus_common::models::homeserver::Homeserver;
use nexus_common::types::DynError;
use pubky::Keypair;
use pubky_app_specs::{PubkyAppUser, PubkyId};

/// Helper: creates a random homeserver in the graph and returns its ID string.
fn random_hs_id() -> String {
    Keypair::random().public_key().to_z32()
}

#[tokio_shared_rt::test(shared)]
async fn test_get_active_homeservers_from_graph_excludes_default_and_orphan() -> Result<(), DynError>
{
    let mut test = WatcherTest::setup().await?;

    // Create an orphan homeserver (no users hosted on it)
    let orphan_keys = Keypair::random();
    let orphan_id = PubkyId::try_from(&orphan_keys.public_key().to_z32())?;
    let orphan_hs = Homeserver::new(orphan_id.clone());
    orphan_hs.put_to_graph().await?;

    // Create a user via WatcherTest, which persists the user in the graph
    let user_kp = Keypair::random();
    let user = PubkyAppUser {
        bio: Some("test_get_active_homeservers_from_graph".to_string()),
        image: None,
        links: None,
        name: "Watcher:AllActiveHS:User".to_string(),
        status: None,
    };
    let user_id = test.create_user(&user_kp, &user).await?;

    // Link the user to the test homeserver via HOSTED_BY
    let default_id = test.homeserver_id.clone();
    let link_query = queries::put::set_user_homeserver(&user_id, &default_id);
    exec_single_row(link_query).await?;

    // Query all active homeservers, excluding the default one
    let hs_ids = Homeserver::get_active_homeservers_from_graph(&default_id)
        .await
        .unwrap();

    // The default homeserver should be excluded
    assert!(
        !hs_ids.contains(&default_id.to_string()),
        "Default HS should be excluded"
    );

    // The orphan homeserver (no active users) should be excluded
    assert!(
        !hs_ids.contains(&orphan_id.to_string()),
        "Orphan HS should be excluded"
    );

    assert!(
        hs_ids.is_empty(),
        "Expected no results: only the default HS as active users, and it is excluded"
    );

    // Cleanup
    test.cleanup_user(&user_kp).await?;

    Ok(())
}

/// Verifies that a non-default homeserver with active users is returned.
#[tokio_shared_rt::test(shared)]
async fn test_active_homeserver_is_returned() -> Result<(), DynError> {
    let mut test = WatcherTest::setup().await?;
    let default_id = test.homeserver_id.clone();

    // Create a second homeserver and link a user to it
    let hs2_id = random_hs_id();
    let hs2 = Homeserver::new(PubkyId::try_from(hs2_id.as_str())?);
    hs2.put_to_graph().await?;

    let user_kp = Keypair::random();
    let user = PubkyAppUser {
        bio: Some("test_active_homeserver_is_returned".to_string()),
        image: None,
        links: None,
        name: "ActiveHS:User".to_string(),
        status: None,
    };
    let user_id = test.create_user(&user_kp, &user).await?;
    exec_single_row(queries::put::set_user_homeserver(&user_id, &hs2_id)).await?;

    let hs_ids = Homeserver::get_active_homeservers_from_graph(&default_id).await?;

    assert!(
        hs_ids.contains(&hs2_id),
        "Non-default HS with an active user should be included"
    );

    // Cleanup
    test.cleanup_user(&user_kp).await?;

    Ok(())
}

/// Verifies that results are sorted by active-user count in descending order.
#[tokio_shared_rt::test(shared)]
async fn test_active_homeservers_sorted_by_user_count() -> Result<(), DynError> {
    let mut test = WatcherTest::setup().await?;
    let default_id = test.homeserver_id.clone();

    // Create two extra homeservers
    let hs_few_id = random_hs_id();
    let hs_many_id = random_hs_id();
    Homeserver::new(PubkyId::try_from(hs_few_id.as_str())?)
        .put_to_graph()
        .await?;
    Homeserver::new(PubkyId::try_from(hs_many_id.as_str())?)
        .put_to_graph()
        .await?;

    // Link 1 user to hs_few
    let kp1 = Keypair::random();
    let user1 = PubkyAppUser {
        bio: None,
        image: None,
        links: None,
        name: "SortTest:Few:1".to_string(),
        status: None,
    };
    let uid1 = test.create_user(&kp1, &user1).await?;
    exec_single_row(queries::put::set_user_homeserver(&uid1, &hs_few_id)).await?;

    // Link 3 users to hs_many
    let mut many_kps = Vec::new();
    for i in 0..3 {
        let kp = Keypair::random();
        let user = PubkyAppUser {
            bio: None,
            image: None,
            links: None,
            name: format!("SortTest:Many:{i}"),
            status: None,
        };
        let uid = test.create_user(&kp, &user).await?;
        exec_single_row(queries::put::set_user_homeserver(&uid, &hs_many_id)).await?;
        many_kps.push(kp);
    }

    let hs_ids = Homeserver::get_active_homeservers_from_graph(&default_id).await?;

    // Both homeservers should be present
    assert!(hs_ids.contains(&hs_few_id), "hs_few should be present");
    assert!(hs_ids.contains(&hs_many_id), "hs_many should be present");

    // hs_many (3 users) should appear before hs_few (1 user) in the results
    let pos_many = hs_ids.iter().position(|id| id == &hs_many_id).unwrap();
    let pos_few = hs_ids.iter().position(|id| id == &hs_few_id).unwrap();
    assert!(
        pos_many < pos_few,
        "HS with more users should appear first (descending order)"
    );

    // Cleanup
    test.cleanup_user(&kp1).await?;
    for kp in &many_kps {
        test.cleanup_user(kp).await?;
    }

    Ok(())
}

/// Verifies that when only orphan homeservers exist (no active users), the
/// result is empty.
#[tokio_shared_rt::test(shared)]
async fn test_only_orphan_homeservers_returns_empty() -> Result<(), DynError> {
    let test = WatcherTest::setup().await?;
    let default_id = test.homeserver_id.clone();

    // Create two orphan homeservers (no users linked)
    for _ in 0..2 {
        let id = random_hs_id();
        Homeserver::new(PubkyId::try_from(id.as_str())?)
            .put_to_graph()
            .await?;
    }

    let hs_ids = Homeserver::get_active_homeservers_from_graph(&default_id).await?;

    // None of the orphan homeservers should appear
    assert!(
        hs_ids.is_empty(),
        "Expected empty result when only orphan homeservers exist"
    );

    Ok(())
}

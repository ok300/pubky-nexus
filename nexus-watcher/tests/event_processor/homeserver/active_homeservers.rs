use crate::event_processor::utils::watcher::WatcherTest;
use anyhow::Result;
use nexus_common::db::exec_single_row;
use nexus_common::db::queries;
use nexus_common::models::homeserver::Homeserver;
use nexus_common::types::DynError;
use pubky::Keypair;
use pubky_app_specs::{PubkyAppUser, PubkyId};

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
        "Expected no results: only the default SH as active users, adn it is excluded"
    );

    // Cleanup
    test.cleanup_user(&user_kp).await?;

    Ok(())
}

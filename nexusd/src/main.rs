use clap::Parser;
use nexus_common::types::DynError;
use nexus_watcher::service::NexusWatcher;
use nexus_webapi::mock::MockDb;
use nexus_webapi::NexusApi;
use nexusd::cli::{
    ApiArgs, Cli, DbCommands, MigrationCommands, N4jCommands, NexusCommands, WatcherArgs,
};
use nexusd::migrations::{import_migrations, MigrationBuilder, MigrationManager};
use nexusd::{DaemonLauncher, N4jOps};

#[tokio::main]
async fn main() -> Result<(), DynError> {
    let cli = Cli::parse();
    let command = Cli::receive_command(cli);
    match command {
        NexusCommands::Db(db_command) => match db_command {
            DbCommands::Clear => MockDb::clear_database().await,
            DbCommands::Mock(args) => MockDb::run(args.mock_type).await,
            DbCommands::Migration(migration_command) => match migration_command {
                MigrationCommands::New(args) => MigrationManager::new_migration(args.name).await?,
                MigrationCommands::Run => {
                    let builder = MigrationBuilder::default().await?;
                    let mut mm = builder.init_stack().await?;
                    import_migrations(&mut mm);
                    mm.run(&builder.migrations_backfill_ready()).await?;
                }
            },
        },
        NexusCommands::N4j(n4j_command) => match n4j_command {
            N4jCommands::Check => N4jOps::check().await?,
            N4jCommands::UserWarmup => N4jOps::user_warmup().await?,
            N4jCommands::FollowsWarmup => N4jOps::follows_warmup().await?,
            N4jCommands::Follows1 => N4jOps::follows_n(1).await?,
            N4jCommands::Follows2 => N4jOps::follows_n(2).await?,
            N4jCommands::Follows3 => N4jOps::follows_n(3).await?,
            N4jCommands::Follows4 => N4jOps::follows_n(4).await?,
            N4jCommands::Follows5 => N4jOps::follows_n(5).await?,
        },
        NexusCommands::Api(ApiArgs { config_dir }) => {
            NexusApi::start_from_daemon(config_dir, None).await?;
        }
        NexusCommands::Watcher(WatcherArgs { config_dir }) => {
            NexusWatcher::start_from_daemon(config_dir, None).await?;
        }
        NexusCommands::Run { config_dir } => {
            DaemonLauncher::start(config_dir, None).await?;
        }
    }

    Ok(())
}

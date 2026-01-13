# Copilot Instructions for Pubky Nexus

## Project Overview

Pubky Nexus is a central bridge connecting Pubky homeservers with social clients. It's a high-performance Rust-based backend that aggregates events from homeservers into a rich social graph, providing a fully featured social-media-like API.

### Architecture Components

- **nexus-webapi**: REST API server using Axum framework for handling client requests
- **nexus-watcher**: Event aggregator that listens to homeserver events and updates the social graph
- **nexus-common**: Shared library containing database connectors, models, and queries
- **nexusd**: Process manager for running Nexus components, handling migrations and reindexing

### Technology Stack

- **Language**: Rust (using workspace with resolver = "2")
- **Web Framework**: Axum for HTTP endpoints
- **Databases**: 
  - Neo4j for graph-based social data
  - Redis for caching and key-value storage
  - PostgreSQL for test data
- **Testing**: cargo-nextest
- **Observability**: OpenTelemetry integration with Signoz
- **API Documentation**: OpenAPI/Swagger with utoipa

## Development Setup

### Prerequisites

Before making any changes, ensure the development environment is properly set up:

1. **Database Setup**: Start required services (Neo4j, Redis, PostgreSQL)
   ```bash
   cd docker
   cp .env-sample .env
   docker compose up -d
   ```

2. **Mock Data**: Load test data before running tests
   ```bash
   cargo run -p nexusd -- db mock
   ```

3. **Environment Variables**: For watcher tests, set:
   ```bash
   export TEST_PUBKY_CONNECTION_STRING=postgres://postgres:postgres@localhost:5432/postgres?pubky-test=true
   ```

### Running the Application

```bash
# Run all services with default config
cargo run -p nexusd

# Run with custom config directory
cargo run -p nexusd -- --config-dir="custom/config/folder"

# Run services individually
cargo run -p nexusd -- watcher
cargo run -p nexusd -- api

# Database operations
cargo run -p nexusd -- db clear
cargo run -p nexusd -- db mock
```

## Build, Test, and Lint

### Formatting

Use `rustfmt` for code formatting:
```bash
cargo fmt           # Format code
cargo fmt -- --check # Check formatting without modifying
```

**Always format code before committing.** The CI will fail if code is not properly formatted.

### Linting

Use `clippy` for linting:
```bash
cargo clippy                  # Run clippy
cargo clippy -- -D warnings   # Fail on warnings (CI requirement)
```

**CI treats all clippy warnings as errors.** Fix all warnings before submitting PRs.

### Testing

Testing requires mock data to be loaded first:

```bash
# Load mock data (required before running tests)
cargo run -p nexusd -- db mock

# Run tests by package
cargo nextest run -p nexus-common --no-fail-fast
cargo nextest run -p nexus-watcher --no-fail-fast
cargo nextest run -p nexus-webapi --no-fail-fast

# Test specific features
cargo nextest run -p nexus-watcher files::create --no-fail-fast
```

**Note**: Always run `cargo run -p nexusd -- db mock` before running tests to ensure the database is in the correct state.

### Benchmarking

```bash
# Run all benchmarks
cargo bench -p nexus-webapi

# Run specific endpoint benchmark
cargo bench -p nexus-webapi --bench user
```

## Code Style and Conventions

### Documentation

- Use Rust doc comments (`//!` for module-level, `///` for item-level)
- Each crate's main library file includes comprehensive module documentation
- Document public APIs, especially complex functions and types
- Include usage examples in documentation where appropriate

### Code Organization

- Follow the workspace structure with separate crates for distinct concerns
- Keep shared code in `nexus-common`
- Use meaningful module hierarchies (e.g., `db/graph/queries/`, `models/`)
- Separate concerns: routes, handlers, models, database logic

### Error Handling

- Use `thiserror` for custom error types
- Use `anyhow` for application-level error handling
- Propagate errors appropriately using `?` operator
- Provide meaningful error messages

### Async/Await

- Use `tokio` runtime with full features
- Mark async functions appropriately
- Use `async-trait` for trait methods when needed
- Follow async best practices for database operations

### Dependencies

- Prefer workspace dependencies defined in root `Cargo.toml`
- Pin dependencies to specific versions or git revisions
- Use feature flags appropriately (e.g., `features = ["openapi"]`)

## Testing Practices

### Test Structure

- Unit tests: Place in the same file using `#[cfg(test)]` modules
- Integration tests: Separate test files in `tests/` directory or package-level tests
- Use `cargo-nextest` for running tests (faster and better output)

### Mock Data

- Mock data is located in `docker/test-graph/mocks`
- Always reload mock data if tests seem out of sync: `cargo run -p nexusd -- db mock`
- Test data includes users, posts, relationships, and other social graph entities

### Test Dependencies

- Tests require Docker services to be running (Neo4j, Redis, PostgreSQL)
- Use environment variables for test configuration
- Ensure proper cleanup in tests to avoid state pollution

## Database Migrations

### Migration System

The project uses a phased migration approach:

1. **Dual Write**: Mirror writes to both old and new sources
2. **Backfill**: Populate new source with historical data
3. **Cutover**: Switch reads to new source
4. **Cleanup**: Remove old source data

### Creating Migrations

```bash
# Generate new migration
cargo run -p nexusd -- db migration new TagCountsReset

# Register in nexusd/src/migrations/mod.rs
# Implement phases in generated file

# Run migrations
cargo run -p nexusd -- db migration run
```

See `examples/migration.rs` for implementation examples.

## Common Pitfalls

1. **Forgetting to load mock data**: Always run `db mock` before tests
2. **Services not running**: Ensure Docker Compose services are up
3. **Environment variables**: Set `TEST_PUBKY_CONNECTION_STRING` for watcher tests
4. **Clippy warnings**: CI fails on warnings; fix them before pushing
5. **Code formatting**: Run `cargo fmt` before committing
6. **Merge conflicts**: Cannot be resolved in this environment; user must handle

## API Development

- API endpoints are defined in `nexus-webapi/src/routes/`
- Use OpenAPI documentation with `utoipa` attributes
- Follow RESTful conventions
- Swagger UI available at `http://localhost:8080/swagger-ui` in development
- API uses `/v0` prefix (currently unstable API)

## Contributing Guidelines

1. **Fork and branch**: Create feature branches from main/dev
2. **Write tests**: Ensure changes are tested and benchmarked
3. **Follow style**: Run `cargo fmt` and fix `cargo clippy` warnings
4. **Document changes**: Update relevant documentation
5. **Submit PRs**: Provide clear descriptions of changes
6. **CI must pass**: All workflows (test, lint, format) must succeed

## Useful Commands Reference

```bash
# Development
cargo run -p nexusd                           # Run all services
cargo run -p nexusd -- watcher                # Run watcher only
cargo run -p nexusd -- api                    # Run API only

# Database
cargo run -p nexusd -- db clear               # Clear database
cargo run -p nexusd -- db mock                # Load mock data
cargo run -p nexusd -- db migration new NAME  # Create migration
cargo run -p nexusd -- db migration run       # Run migrations

# Quality checks
cargo fmt                                      # Format code
cargo fmt -- --check                           # Check formatting
cargo clippy -- -D warnings                    # Lint with error on warnings

# Testing
cargo nextest run -p nexus-common --no-fail-fast
cargo nextest run -p nexus-watcher --no-fail-fast
cargo nextest run -p nexus-webapi --no-fail-fast

# Benchmarking
cargo bench -p nexus-webapi                    # All benchmarks
cargo bench -p nexus-webapi --bench user       # Specific benchmark
```

## Observability

- Traces and metrics available via OpenTelemetry
- Configure `otlp_endpoint` in config.toml to enable
- Access Signoz dashboard at `http://localhost:3301` when configured
- Redis Insight: `http://localhost:8001/redis-stack/browser`
- Neo4j Browser: `http://localhost:7474/browser/`

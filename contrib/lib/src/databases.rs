//! Traits, utilities, and a macro for easy database connection pooling.
//!
//! # Overview
//!
//! This module provides traits, utilities, and a procedural macro that allows
//! you to easily connect your Rocket application to databases through
//! connection pools. A _database connection pool_ is a data structure that
//! maintains active database connections for later use in the application.
//! This implementation of connection pooling support is based on
//! [`r2d2`] and exposes connections through [request guards]. Databases are
//! individually configured through Rocket's regular configuration mechanisms: a
//! `Rocket.toml` file, environment variables, or procedurally.
//!
//! Connecting your Rocket application to a database using this library occurs
//! in three simple steps:
//!
//!   1. Configure your databases in `Rocket.toml`.
//!      (see [Configuration](#configuration))
//!   2. Associate a request guard type and fairing with each database.
//!      (see [Guard Types](#guard-types))
//!   3. Use the request guard to retrieve a connection in a handler.
//!      (see [Handlers](#handlers))
//!
//! For a list of supported databases, see [Provided Databases](#provided). This
//! support can be easily extended by implementing the [`Poolable`] trait. See
//! [Extending](#extending) for more.
//!
//! ## Example
//!
//! Before using this library, the feature corresponding to your database type
//! in `rocket_contrib` must be enabled:
//!
//! ```toml
//! [dependencies.rocket_contrib]
//! version = "0.5.0-dev"
//! default-features = false
//! features = ["diesel_sqlite_pool"]
//! ```
//!
//! See [Provided](#provided) for a list of supported database and their
//! associated feature name.
//!
//! In `Rocket.toml` or the equivalent via environment variables:
//!
//! ```toml
//! [global.databases]
//! sqlite_logs = { url = "/path/to/database.sqlite" }
//! ```
//!
//! In your application's source code, one-time:
//!
//! ```rust
//! #[macro_use] extern crate rocket;
//! #[macro_use] extern crate rocket_contrib;
//!
//! # #[cfg(feature = "diesel_sqlite_pool")]
//! # mod test {
//! use rocket_contrib::databases::diesel;
//!
//! #[database("sqlite_logs")]
//! struct LogsDbConn(diesel::SqliteConnection);
//!
//! #[launch]
//! fn rocket() -> rocket::Rocket {
//!     rocket::ignite().attach(LogsDbConn::fairing())
//! }
//! # } fn main() {}
//! ```
//!
//! Whenever a connection to the database is needed:
//!
//! ```rust
//! # #[macro_use] extern crate rocket;
//! # #[macro_use] extern crate rocket_contrib;
//! #
//! # #[cfg(feature = "diesel_sqlite_pool")]
//! # mod test {
//! # use rocket_contrib::databases::diesel;
//! #
//! # #[database("sqlite_logs")]
//! # struct LogsDbConn(diesel::SqliteConnection);
//! #
//! # type Logs = ();
//! # type Result<T> = std::result::Result<T, ()>;
//! #
//! #[get("/logs/<id>")]
//! async fn get_logs(conn: LogsDbConn, id: usize) -> Result<Logs> {
//! # /*
//!     conn.run(|c| Logs::by_id(c, id)).await
//! # */
//! # Ok(())
//! }
//! # } fn main() {}
//! ```
//!
//! # Usage
//!
//! ## Configuration
//!
//! Databases can be configured via various mechanisms: `Rocket.toml`,
//! procedurally via `rocket::custom()`, or via environment variables.
//!
//! ### `Rocket.toml`
//!
//! To configure a database via `Rocket.toml`, add a table for each database
//! to the `databases` table where the key is a name of your choice. The table
//! should have a `url` key and, optionally, a `pool_size` key. This looks as
//! follows:
//!
//! ```toml
//! # Option 1:
//! [global.databases]
//! sqlite_db = { url = "db.sqlite" }
//!
//! # Option 2:
//! [global.databases.my_db]
//! url = "mysql://root:root@localhost/my_db"
//!
//! # With a `pool_size` key:
//! [global.databases]
//! sqlite_db = { url = "db.sqlite", pool_size = 20 }
//! ```
//!
//! The table _requires_ one key:
//!
//!   * `url` - the URl to the database
//!
//! Additionally, all configurations accept the following _optional_ keys:
//!
//!   * `pool_size` - the size of the pool, i.e., the number of connections to
//!     pool (defaults to the configured number of workers)
//!
//! Additional options may be required or supported by other adapters.
//!
//! ### Procedurally
//!
//! Databases can also be configured procedurally via `rocket::custom()`.
//! The example below does just this:
//!
//! ```rust
//! #[macro_use] extern crate rocket;
//!
//! # #[cfg(feature = "diesel_sqlite_pool")]
//! # mod test {
//! use std::collections::HashMap;
//! use rocket::config::{Config, Environment, Value};
//!
//! #[launch]
//! fn rocket() -> rocket::Rocket {
//!     let mut database_config = HashMap::new();
//!     let mut databases = HashMap::new();
//!
//!     // This is the same as the following TOML:
//!     // my_db = { url = "database.sqlite" }
//!     database_config.insert("url", Value::from("database.sqlite"));
//!     databases.insert("my_db", Value::from(database_config));
//!
//!     let config = Config::build(Environment::Development)
//!         .extra("databases", databases)
//!         .finalize()
//!         .unwrap();
//!
//!     rocket::custom(config)
//! }
//! # } fn main() {}
//! ```
//!
//! ### Environment Variables
//!
//! Lastly, databases can be configured via environment variables by specifying
//! the `databases` table as detailed in the [Environment Variables
//! configuration
//! guide](https://rocket.rs/v0.5/guide/configuration/#environment-variables):
//!
//! ```bash
//! ROCKET_DATABASES='{my_db={url="db.sqlite"}}'
//! ```
//!
//! Multiple databases can be specified in the `ROCKET_DATABASES` environment variable
//! as well by comma separating them:
//!
//! ```bash
//! ROCKET_DATABASES='{my_db={url="db.sqlite"},my_pg_db={url="postgres://root:root@localhost/my_pg_db"}}'
//! ```
//!
//! ## Guard Types
//!
//! Once a database has been configured, the `#[database]` attribute can be used
//! to tie a type in your application to a configured database. The database
//! attributes accepts a single string parameter that indicates the name of the
//! database. This corresponds to the database name set as the database's
//! configuration key.
//!
//! The macro generates a [`FromRequest`] implementation for the decorated type,
//! allowing the type to be used as a request guard. This implementation
//! retrieves a connection from the database pool or fails with a
//! `Status::ServiceUnavailable` if connecting to the database times out.
//!
//! The macro will also generate two inherent methods on the decorated type:
//!
//!   * `fn fairing() -> impl Fairing`
//!
//!      Returns a fairing that initializes the associated database connection
//!      pool.
//!
//!   * `async fn get_one(&Cargo) -> Option<Self>`
//!
//!     Retrieves a connection wrapper from the configured pool. Returns `Some`
//!     as long as `Self::fairing()` has been attached.
//!
//! The attribute can only be applied to unit-like structs with one type. The
//! internal type of the structure must implement [`Poolable`].
//!
//! ```rust
//! # extern crate rocket;
//! # #[macro_use] extern crate rocket_contrib;
//! # #[cfg(feature = "diesel_sqlite_pool")]
//! # mod test {
//! use rocket_contrib::databases::diesel;
//!
//! #[database("my_db")]
//! struct MyDatabase(diesel::SqliteConnection);
//! # }
//! ```
//!
//! Other databases can be used by specifying their respective [`Poolable`]
//! type:
//!
//! ```rust
//! # extern crate rocket;
//! # #[macro_use] extern crate rocket_contrib;
//! # #[cfg(feature = "postgres_pool")]
//! # mod test {
//! use rocket_contrib::databases::postgres;
//!
//! #[database("my_pg_db")]
//! struct MyPgDatabase(postgres::Client);
//! # }
//! ```
//!
//! The fairing returned from the generated `fairing()` method _must_ be
//! attached for the request guard implementation to succeed. Putting the pieces
//! together, a use of the `#[database]` attribute looks as follows:
//!
//! ```rust
//! # #[macro_use] extern crate rocket;
//! # #[macro_use] extern crate rocket_contrib;
//! #
//! # #[cfg(feature = "diesel_sqlite_pool")]
//! # mod test {
//! # use std::collections::HashMap;
//! # use rocket::config::{Config, Environment, Value};
//! #
//! use rocket_contrib::databases::diesel;
//!
//! #[database("my_db")]
//! struct MyDatabase(diesel::SqliteConnection);
//!
//! #[launch]
//! fn rocket() -> rocket::Rocket {
//! #     let mut db_config = HashMap::new();
//! #     let mut databases = HashMap::new();
//! #
//! #     db_config.insert("url", Value::from("database.sqlite"));
//! #     db_config.insert("pool_size", Value::from(10));
//! #     databases.insert("my_db", Value::from(db_config));
//! #
//! #     let config = Config::build(Environment::Development)
//! #         .extra("databases", databases)
//! #         .finalize()
//! #         .unwrap();
//! #
//!     rocket::custom(config).attach(MyDatabase::fairing())
//! }
//! # } fn main() {}
//! ```
//!
//! ## Handlers
//!
//! Finally, use your type as a request guard in a handler to retrieve a
//! connection wrapper for the database:
//!
//! ```rust
//! # #[macro_use] extern crate rocket;
//! # #[macro_use] extern crate rocket_contrib;
//! #
//! # #[cfg(feature = "diesel_sqlite_pool")]
//! # mod test {
//! # use rocket_contrib::databases::diesel;
//! #[database("my_db")]
//! struct MyDatabase(diesel::SqliteConnection);
//!
//! #[get("/")]
//! fn my_handler(conn: MyDatabase) {
//!     // ...
//! }
//! # }
//! ```
//!
//! A connection can be retrieved and used with the `run()` method:
//!
//! ```rust
//! # #[macro_use] extern crate rocket;
//! # #[macro_use] extern crate rocket_contrib;
//! #
//! # #[cfg(feature = "diesel_sqlite_pool")]
//! # mod test {
//! # use rocket_contrib::databases::diesel;
//! # type Data = ();
//! #[database("my_db")]
//! struct MyDatabase(diesel::SqliteConnection);
//!
//! fn load_from_db(conn: &diesel::SqliteConnection) -> Data {
//!     // Do something with connection, return some data.
//!     # ()
//! }
//!
//! #[get("/")]
//! async fn my_handler(mut conn: MyDatabase) -> Data {
//!     conn.run(|c| load_from_db(c)).await
//! }
//! # }
//! ```
//!
//! # Database Support
//!
//! Built-in support is provided for many popular databases and drivers. Support
//! can be easily extended by [`Poolable`] implementations.
//!
//! ## Provided
//!
//! The list below includes all presently supported database adapters and their
//! corresponding [`Poolable`] type.
//!
// Note: Keep this table in sync with site/guite/6-state.md
//! | Kind     | Driver                | Version   | `Poolable` Type                | Feature                |
//! |----------|-----------------------|-----------|--------------------------------|------------------------|
//! | MySQL    | [Diesel]              | `1`       | [`diesel::MysqlConnection`]    | `diesel_mysql_pool`    |
//! | MySQL    | [`rust-mysql-simple`] | `18`      | [`mysql::Conn`]                | `mysql_pool`           |
//! | Postgres | [Diesel]              | `1`       | [`diesel::PgConnection`]       | `diesel_postgres_pool` |
//! | Postgres | [Rust-Postgres]       | `0.17`    | [`postgres::Client`]           | `postgres_pool`        |
//! | Sqlite   | [Diesel]              | `1`       | [`diesel::SqliteConnection`]   | `diesel_sqlite_pool`   |
//! | Sqlite   | [`Rusqlite`]          | `0.23`    | [`rusqlite::Connection`]       | `sqlite_pool`          |
//! | Memcache | [`memcache`]          | `0.14`    | [`memcache::Client`]           | `memcache_pool`        |
//!
//! [Diesel]: https://diesel.rs
//! [`rusqlite::Connection`]: https://docs.rs/rusqlite/0.23.0/rusqlite/struct.Connection.html
//! [`diesel::SqliteConnection`]: http://docs.diesel.rs/diesel/prelude/struct.SqliteConnection.html
//! [`postgres::Client`]: https://docs.rs/postgres/0.17/postgres/struct.Client.html
//! [`diesel::PgConnection`]: http://docs.diesel.rs/diesel/pg/struct.PgConnection.html
//! [`mysql::Conn`]: https://docs.rs/mysql/18/mysql/struct.Conn.html
//! [`diesel::MysqlConnection`]: http://docs.diesel.rs/diesel/mysql/struct.MysqlConnection.html
//! [`Rusqlite`]: https://github.com/jgallagher/rusqlite
//! [Rust-Postgres]: https://github.com/sfackler/rust-postgres
//! [`rust-mysql-simple`]: https://github.com/blackbeam/rust-mysql-simple
//! [`diesel::PgConnection`]: http://docs.diesel.rs/diesel/pg/struct.PgConnection.html
//! [`memcache`]: https://github.com/aisk/rust-memcache
//! [`memcache::Client`]: https://docs.rs/memcache/0.14/memcache/struct.Client.html
//!
//! The above table lists all the supported database adapters in this library.
//! In order to use particular `Poolable` type that's included in this library,
//! you must first enable the feature listed in the "Feature" column. The
//! interior type of your decorated database type should match the type in the
//! "`Poolable` Type" column.
//!
//! ## Extending
//!
//! Extending Rocket's support to your own custom database adapter (or other
//! database-like struct that can be pooled by `r2d2`) is as easy as
//! implementing the [`Poolable`] trait. See the documentation for [`Poolable`]
//! for more details on how to implement it.
//!
//! [`FromRequest`]: rocket::request::FromRequest
//! [request guards]: rocket::request::FromRequest
//! [`Poolable`]: crate::databases::Poolable

pub extern crate r2d2;

#[cfg(any(feature = "diesel_sqlite_pool",
          feature = "diesel_postgres_pool",
          feature = "diesel_mysql_pool"))]
pub extern crate diesel;

use std::fmt::{self, Display, Formatter};
use std::marker::PhantomData;
use std::sync::Arc;

use rocket::config::{self, Value};
use rocket::fairing::{AdHoc, Fairing};
use rocket::request::{Request, Outcome, FromRequest};
use rocket::outcome::IntoOutcome;
use rocket::http::Status;

use rocket::tokio::sync::{OwnedSemaphorePermit, Semaphore, Mutex};

use self::r2d2::ManageConnection;

#[doc(hidden)] pub use rocket_contrib_codegen::*;

#[cfg(feature = "postgres_pool")] pub extern crate postgres;
#[cfg(feature = "postgres_pool")] pub extern crate r2d2_postgres;

#[cfg(feature = "mysql_pool")] pub extern crate mysql;
#[cfg(feature = "mysql_pool")] pub extern crate r2d2_mysql;

#[cfg(feature = "sqlite_pool")] pub extern crate rusqlite;
#[cfg(feature = "sqlite_pool")] pub extern crate r2d2_sqlite;

#[cfg(feature = "memcache_pool")] pub extern crate memcache;
#[cfg(feature = "memcache_pool")] pub extern crate r2d2_memcache;

/// A structure representing a particular database configuration.
///
/// For the following configuration:
///
/// ```toml
/// [global.databases.my_database]
/// url = "postgres://root:root@localhost/my_database"
/// pool_size = 10
/// certs = "sample_cert.pem"
/// key = "key.pem"
/// ```
///
/// The following structure would be generated after calling
/// [`database_config`]`("my_database", &config)`:
///
/// ```rust,ignore
/// DatabaseConfig {
///     url: "dummy_db.sqlite",
///     pool_size: 10,
///     extras: {
///         "certs": String("certs.pem"),
///         "key": String("key.pem"),
///     },
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseConfig<'a> {
    /// The connection URL specified in the Rocket configuration.
    pub url: &'a str,
    /// The size of the pool to be initialized. Defaults to the number of
    /// Rocket workers.
    pub pool_size: u32,
    /// Any extra options that are included in the configuration, **excluding**
    /// the url and pool_size.
    pub extras: rocket::config::Map<String, Value>,
}

/// A wrapper around `r2d2::Error`s or a custom database error type.
///
/// This type is only relevant to implementors of the [`Poolable`] trait. See
/// the [`Poolable`] documentation for more information on how to use this type.
#[derive(Debug)]
pub enum DbError<T> {
    /// The custom error type to wrap alongside `r2d2::Error`.
    Custom(T),
    /// The error returned by an r2d2 pool.
    PoolError(r2d2::Error),
}

/// Error returned on invalid database configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// The `databases` configuration key is missing or is empty.
    MissingTable,
    /// The requested database configuration key is missing from the active
    /// configuration.
    MissingKey,
    /// The configuration associated with the key isn't a
    /// [`Table`](rocket::config::Table).
    MalformedConfiguration,
    /// The required `url` key is missing.
    MissingUrl,
    /// The value for `url` isn't a string.
    MalformedUrl,
    /// The `pool_size` exceeds `u32::max_value()` or is negative.
    InvalidPoolSize(i64),
}

/// Retrieves the database configuration for the database named `name`.
///
/// This function is primarily used by the code generated by the `#[database]`
/// attribute.
///
/// # Example
///
/// Consider the following configuration:
///
/// ```toml
/// [global.databases]
/// my_db = { url = "db/db.sqlite", pool_size = 25 }
/// my_other_db = { url = "mysql://root:root@localhost/database" }
/// ```
///
/// The following example uses `database_config` to retrieve the configurations
/// for the `my_db` and `my_other_db` databases:
///
/// ```rust
/// # extern crate rocket;
/// # extern crate rocket_contrib;
/// #
/// # use std::{collections::BTreeMap, mem::drop};
/// # use rocket::{fairing::AdHoc, config::{Config, Environment, Value}};
/// use rocket_contrib::databases::{database_config, ConfigError};
///
/// # let mut databases = BTreeMap::new();
/// #
/// # let mut my_db = BTreeMap::new();
/// # my_db.insert("url".to_string(), Value::from("db/db.sqlite"));
/// # my_db.insert("pool_size".to_string(), Value::from(25));
/// #
/// # let mut my_other_db = BTreeMap::new();
/// # my_other_db.insert("url".to_string(),
/// #     Value::from("mysql://root:root@localhost/database"));
/// #
/// # databases.insert("my_db".to_string(), Value::from(my_db));
/// # databases.insert("my_other_db".to_string(), Value::from(my_other_db));
/// #
/// # let config = Config::build(Environment::Development)
/// #     .extra("databases", databases)
/// #     .expect("custom config okay");
/// #
/// # rocket::custom(config).attach(AdHoc::on_attach("Testing", |mut rocket| async {
/// # {
/// let rocket_config = rocket.config().await;
/// let config = database_config("my_db", rocket_config).unwrap();
/// assert_eq!(config.url, "db/db.sqlite");
/// assert_eq!(config.pool_size, 25);
///
/// let other_config = database_config("my_other_db", rocket_config).unwrap();
/// assert_eq!(other_config.url, "mysql://root:root@localhost/database");
///
/// let error = database_config("invalid_db", rocket_config).unwrap_err();
/// assert_eq!(error, ConfigError::MissingKey);
/// # }
/// #
/// #     Ok(rocket)
/// # }));
/// ```
pub fn database_config<'a>(
    name: &str,
    from: &'a config::Config
) -> Result<DatabaseConfig<'a>, ConfigError> {
    // Find the first `databases` config that's a table with a key of 'name'
    // equal to `name`.
    let connection_config = from.get_table("databases")
        .map_err(|_| ConfigError::MissingTable)?
        .get(name)
        .ok_or(ConfigError::MissingKey)?
        .as_table()
        .ok_or(ConfigError::MalformedConfiguration)?;

    let maybe_url = connection_config.get("url")
        .ok_or(ConfigError::MissingUrl)?;

    let url = maybe_url.as_str().ok_or(ConfigError::MalformedUrl)?;

    let pool_size = connection_config.get("pool_size")
        .and_then(Value::as_integer)
        .unwrap_or(from.workers as i64);

    if pool_size < 1 || pool_size > u32::max_value() as i64 {
        return Err(ConfigError::InvalidPoolSize(pool_size));
    }

    let mut extras = connection_config.clone();
    extras.remove("url");
    extras.remove("pool_size");

    Ok(DatabaseConfig { url, pool_size: pool_size as u32, extras: extras })
}

impl<'a> Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingTable => {
                write!(f, "A table named `databases` was not found for this configuration")
            },
            ConfigError::MissingKey => {
                write!(f, "An entry in the `databases` table was not found for this key")
            },
            ConfigError::MalformedConfiguration => {
                write!(f, "The configuration for this database is malformed")
            }
            ConfigError::MissingUrl => {
                write!(f, "The connection URL is missing for this database")
            },
            ConfigError::MalformedUrl => {
                write!(f, "The specified connection URL is malformed")
            },
            ConfigError::InvalidPoolSize(invalid_size) => {
                write!(f, "'{}' is not a valid value for `pool_size`", invalid_size)
            },
        }
    }
}

/// Trait implemented by `r2d2`-based database adapters.
///
/// # Provided Implementations
///
/// Implementations of `Poolable` are provided for the following types:
///
///   * `diesel::MysqlConnection`
///   * `diesel::PgConnection`
///   * `diesel::SqliteConnection`
///   * `postgres::Connection`
///   * `mysql::Conn`
///   * `rusqlite::Connection`
///
/// # Implementation Guide
///
/// As a r2d2-compatible database (or other resource) adapter provider,
/// implementing `Poolable` in your own library will enable Rocket users to
/// consume your adapter with its built-in connection pooling support.
///
/// ## Example
///
/// Consider a library `foo` with the following types:
///
///   * `foo::ConnectionManager`, which implements [`r2d2::ManageConnection`]
///   * `foo::Connection`, the `Connection` associated type of
///     `foo::ConnectionManager`
///   * `foo::Error`, errors resulting from manager instantiation
///
/// In order for Rocket to generate the required code to automatically provision
/// a r2d2 connection pool into application state, the `Poolable` trait needs to
/// be implemented for the connection type. The following example implements
/// `Poolable` for `foo::Connection`:
///
/// ```rust
/// use rocket_contrib::databases::{r2d2, DbError, DatabaseConfig, Poolable};
/// # mod foo {
/// #     use std::fmt;
/// #     use rocket_contrib::databases::r2d2;
/// #     #[derive(Debug)] pub struct Error;
/// #     impl std::error::Error for Error {  }
/// #     impl fmt::Display for Error {
/// #         fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) }
/// #     }
/// #
/// #     pub struct Connection;
/// #     pub struct ConnectionManager;
/// #
/// #     type Result<T> = std::result::Result<T, Error>;
/// #
/// #     impl ConnectionManager {
/// #         pub fn new(url: &str) -> Result<Self> { Err(Error) }
/// #     }
/// #
/// #     impl self::r2d2::ManageConnection for ConnectionManager {
/// #          type Connection = Connection;
/// #          type Error = Error;
/// #          fn connect(&self) -> Result<Connection> { panic!(()) }
/// #          fn is_valid(&self, _: &mut Connection) -> Result<()> { panic!() }
/// #          fn has_broken(&self, _: &mut Connection) -> bool { panic!() }
/// #     }
/// # }
/// #
/// impl Poolable for foo::Connection {
///     type Manager = foo::ConnectionManager;
///     type Error = DbError<foo::Error>;
///
///     fn pool(config: DatabaseConfig) -> Result<r2d2::Pool<Self::Manager>, Self::Error> {
///         let manager = foo::ConnectionManager::new(config.url)
///             .map_err(DbError::Custom)?;
///
///         r2d2::Pool::builder()
///             .max_size(config.pool_size)
///             .build(manager)
///             .map_err(DbError::PoolError)
///     }
/// }
/// ```
///
/// In this example, `ConnectionManager::new()` method returns a `foo::Error` on
/// failure. For convenience, the [`DbError`] enum is used to consolidate this
/// error type and the `r2d2::Error` type that can result from
/// `r2d2::Pool::builder()`.
///
/// In the event that a connection manager isn't fallible (as is the case with
/// Diesel's r2d2 connection manager, for instance), the associated error type
/// for the `Poolable` implementation can simply be `r2d2::Error` as this is the
/// only error that can be result. For more concrete example, consult Rocket's
/// existing implementations of [`Poolable`].
pub trait Poolable: Send + Sized + 'static {
    /// The associated connection manager for the given connection type.
    type Manager: ManageConnection<Connection=Self>;
    /// The associated error type in the event that constructing the connection
    /// manager and/or the connection pool fails.
    type Error: std::fmt::Debug;

    /// Creates an `r2d2` connection pool for `Manager::Connection`, returning
    /// the pool on success.
    fn pool(config: DatabaseConfig<'_>) -> Result<r2d2::Pool<Self::Manager>, Self::Error>;
}

#[cfg(feature = "diesel_sqlite_pool")]
impl Poolable for diesel::SqliteConnection {
    type Manager = diesel::r2d2::ConnectionManager<diesel::SqliteConnection>;
    type Error = r2d2::Error;

    fn pool(config: DatabaseConfig<'_>) -> Result<r2d2::Pool<Self::Manager>, Self::Error> {
        let manager = diesel::r2d2::ConnectionManager::new(config.url);
        r2d2::Pool::builder().max_size(config.pool_size).build(manager)
    }
}

#[cfg(feature = "diesel_postgres_pool")]
impl Poolable for diesel::PgConnection {
    type Manager = diesel::r2d2::ConnectionManager<diesel::PgConnection>;
    type Error = r2d2::Error;

    fn pool(config: DatabaseConfig<'_>) -> Result<r2d2::Pool<Self::Manager>, Self::Error> {
        let manager = diesel::r2d2::ConnectionManager::new(config.url);
        r2d2::Pool::builder().max_size(config.pool_size).build(manager)
    }
}

#[cfg(feature = "diesel_mysql_pool")]
impl Poolable for diesel::MysqlConnection {
    type Manager = diesel::r2d2::ConnectionManager<diesel::MysqlConnection>;
    type Error = r2d2::Error;

    fn pool(config: DatabaseConfig<'_>) -> Result<r2d2::Pool<Self::Manager>, Self::Error> {
        let manager = diesel::r2d2::ConnectionManager::new(config.url);
        r2d2::Pool::builder().max_size(config.pool_size).build(manager)
    }
}

// TODO: Come up with a way to handle TLS
#[cfg(feature = "postgres_pool")]
impl Poolable for postgres::Client {
    type Manager = r2d2_postgres::PostgresConnectionManager<postgres::tls::NoTls>;
    type Error = DbError<postgres::Error>;

    fn pool(config: DatabaseConfig<'_>) -> Result<r2d2::Pool<Self::Manager>, Self::Error> {
        let manager = r2d2_postgres::PostgresConnectionManager::new(
            config.url.parse().map_err(DbError::Custom)?,
            postgres::tls::NoTls,
        );

        r2d2::Pool::builder().max_size(config.pool_size).build(manager)
            .map_err(DbError::PoolError)
    }
}

#[cfg(feature = "mysql_pool")]
impl Poolable for mysql::Conn {
    type Manager = r2d2_mysql::MysqlConnectionManager;
    type Error = r2d2::Error;

    fn pool(config: DatabaseConfig<'_>) -> Result<r2d2::Pool<Self::Manager>, Self::Error> {
        let opts = mysql::OptsBuilder::from_opts(config.url);
        let manager = r2d2_mysql::MysqlConnectionManager::new(opts);
        r2d2::Pool::builder().max_size(config.pool_size).build(manager)
    }
}

#[cfg(feature = "sqlite_pool")]
impl Poolable for rusqlite::Connection {
    type Manager = r2d2_sqlite::SqliteConnectionManager;
    type Error = r2d2::Error;

    fn pool(config: DatabaseConfig<'_>) -> Result<r2d2::Pool<Self::Manager>, Self::Error> {
        let manager = r2d2_sqlite::SqliteConnectionManager::file(config.url);

        r2d2::Pool::builder().max_size(config.pool_size).build(manager)
    }
}

#[cfg(feature = "memcache_pool")]
impl Poolable for memcache::Client {
    type Manager = r2d2_memcache::MemcacheConnectionManager;
    type Error = DbError<memcache::MemcacheError>;

    fn pool(config: DatabaseConfig<'_>) -> Result<r2d2::Pool<Self::Manager>, Self::Error> {
        let manager = r2d2_memcache::MemcacheConnectionManager::new(config.url);
        r2d2::Pool::builder().max_size(config.pool_size).build(manager).map_err(DbError::PoolError)
    }
}

/// Unstable internal details of generated code for the #[database] attribute.
///
/// This type is implemented here instead of in generated code to ensure all
/// types are properly checked.
#[doc(hidden)]
pub struct ConnectionPool<K, C: Poolable> {
    pool: r2d2::Pool<C::Manager>,
    semaphore: Arc<Semaphore>,
    _marker: PhantomData<fn() -> K>,
}

/// Unstable internal details of generated code for the #[database] attribute.
///
/// This type is implemented here instead of in generated code to ensure all
/// types are properly checked.
#[doc(hidden)]
pub struct Connection<K, C: Poolable> {
    connection: Arc<Mutex<Option<r2d2::PooledConnection<C::Manager>>>>,
    permit: Option<OwnedSemaphorePermit>,
    _marker: PhantomData<fn() -> K>,
}

// A wrapper around spawn_blocking that propagates panics to the calling code.
async fn run_blocking<F, R>(job: F) -> R
    where F: FnOnce() -> R + Send + 'static, R: Send + 'static,
{
    match tokio::task::spawn_blocking(job).await {
        Ok(ret) => ret,
        Err(e) => match e.try_into_panic() {
            Ok(panic) => std::panic::resume_unwind(panic),
            Err(_) => unreachable!("spawn_blocking tasks are never cancelled"),
        }
    }
}

impl<K: 'static, C: Poolable> ConnectionPool<K, C> {
    pub fn fairing(fairing_name: &'static str, config_name: &'static str) -> impl Fairing {
        AdHoc::on_attach(fairing_name, move |mut rocket| async move {
            let config = database_config(config_name, rocket.config().await);
            let pool = config.map(|c| (c.pool_size, C::pool(c)));

            match pool {
                Ok((size, Ok(pool))) => {
                    let managed = ConnectionPool::<K, C> {
                        pool,
                        semaphore: Arc::new(Semaphore::new(size as usize)),
                        _marker: PhantomData,
                    };
                    Ok(rocket.manage(managed))
                },
                Err(config_error) => {
                    rocket::logger::error(
                        &format!("Database configuration failure: '{}'", config_name));
                    rocket::logger::error_(&config_error.to_string());
                    Err(rocket)
                },
                Ok((_, Err(pool_error))) => {
                    rocket::logger::error(
                        &format!("Failed to initialize pool for '{}'", config_name));
                    rocket::logger::error_(&format!("{:?}", pool_error));
                    Err(rocket)
                },
            }
        })
    }

    async fn get(&self) -> Result<Connection<K, C>, ()> {
        // TODO: Make timeout configurable.
        let permit = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.semaphore.clone().acquire_owned()
        ).await {
            Ok(p) => p,
            Err(_) => {
                error_!("Failed to get a database connection within the timeout.");
                return Err(());
            }
        };

        // TODO: Make timeout configurable.
        let pool = self.pool.clone();
        match run_blocking(move || pool.get_timeout(std::time::Duration::from_secs(5))).await {
            Ok(c) => Ok(Connection {
                connection: Arc::new(Mutex::new(Some(c))),
                permit: Some(permit),
                _marker: PhantomData,
            }),
            Err(e) => {
                error_!("Failed to get a database connection: {}", e);
                Err(())
            }
        }
    }

    #[inline]
    pub async fn get_one(cargo: &rocket::Cargo) -> Option<Connection<K, C>> {
        match cargo.state::<Self>() {
            Some(pool) => pool.get().await.ok(),
            None => {
                error_!("Database fairing was not attached for {}", std::any::type_name::<K>());
                None
            }
        }
    }
}

impl<K: 'static, C: Poolable> Connection<K, C> {
    #[inline]
    pub async fn run<F, R>(&self, f: F) -> R
        where F: FnOnce(&mut C) -> R + Send + 'static,
              R: Send + 'static,
    {
        let mut connection = self.connection.clone().lock_owned().await;
        run_blocking(move || {
            let conn = connection.as_mut()
                .expect("internal invariant broken: self.connection is Some");
            f(conn)
        }).await
    }
}

impl<K, C: Poolable> Drop for Connection<K, C> {
    fn drop(&mut self) {
        let connection = self.connection.clone();
        let permit = self.permit.take();
        tokio::spawn(async move {
            let mut connection = connection.lock_owned().await;
            tokio::task::spawn_blocking(move || {
                if let Some(conn) = connection.take() {
                    drop(conn);
                }
                // NB: Explicitly dropping the permit here so that it's only
                // released after the connection is.
                drop(permit);
            })
        });
    }
}

#[rocket::async_trait]
impl<'a, 'r, K: 'static, C: Poolable> FromRequest<'a, 'r> for Connection<K, C> {
    type Error = ();

    #[inline]
    async fn from_request(request: &'a Request<'r>) -> Outcome<Self, ()> {
        match request.managed_state::<ConnectionPool<K, C>>() {
            Some(c) => c.get().await.into_outcome(Status::ServiceUnavailable),
            None => {
                error_!("Missing database fairing for `{}`", std::any::type_name::<K>());
                Outcome::Failure((Status::InternalServerError, ()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use rocket::{Config, config::{Environment, Value}};
    use super::{ConfigError::*, database_config};

    #[test]
    fn no_database_entry_in_config_returns_error() {
        let config = Config::build(Environment::Development)
            .finalize()
            .unwrap();
        let database_config_result = database_config("dummy_db", &config);

        assert_eq!(Err(MissingTable), database_config_result);
    }

    #[test]
    fn no_matching_connection_returns_error() {
        // Laboriously setup the config extras
        let mut database_extra = BTreeMap::new();
        let mut connection_config = BTreeMap::new();
        connection_config.insert("url".to_string(), Value::from("dummy_db.sqlite"));
        connection_config.insert("pool_size".to_string(), Value::from(10));
        database_extra.insert("dummy_db".to_string(), Value::from(connection_config));

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config_result = database_config("real_db", &config);

        assert_eq!(Err(MissingKey), database_config_result);
    }

    #[test]
    fn incorrectly_structured_config_returns_error() {
        let mut database_extra = BTreeMap::new();
        let connection_config = vec!["url", "dummy_db.slqite"];
        database_extra.insert("dummy_db".to_string(), Value::from(connection_config));

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config_result = database_config("dummy_db", &config);

        assert_eq!(Err(MalformedConfiguration), database_config_result);
    }

    #[test]
    fn missing_connection_string_returns_error() {
        let mut database_extra = BTreeMap::new();
        let connection_config: BTreeMap<String, Value> = BTreeMap::new();
        database_extra.insert("dummy_db", connection_config);

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config_result = database_config("dummy_db", &config);

        assert_eq!(Err(MissingUrl), database_config_result);
    }

    #[test]
    fn invalid_connection_string_returns_error() {
        let mut database_extra = BTreeMap::new();
        let mut connection_config = BTreeMap::new();
        connection_config.insert("url".to_string(), Value::from(42));
        database_extra.insert("dummy_db", connection_config);

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config_result = database_config("dummy_db", &config);

        assert_eq!(Err(MalformedUrl), database_config_result);
    }

    #[test]
    fn negative_pool_size_returns_error() {
        let mut database_extra = BTreeMap::new();
        let mut connection_config = BTreeMap::new();
        connection_config.insert("url".to_string(), Value::from("dummy_db.sqlite"));
        connection_config.insert("pool_size".to_string(), Value::from(-1));
        database_extra.insert("dummy_db", connection_config);

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config_result = database_config("dummy_db", &config);

        assert_eq!(Err(InvalidPoolSize(-1)), database_config_result);
    }

    #[test]
    fn pool_size_beyond_u32_max_returns_error() {
        let mut database_extra = BTreeMap::new();
        let mut connection_config = BTreeMap::new();
        let over_max = (u32::max_value()) as i64 + 1;
        connection_config.insert("url".to_string(), Value::from("dummy_db.sqlite"));
        connection_config.insert("pool_size".to_string(), Value::from(over_max));
        database_extra.insert("dummy_db", connection_config);

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config_result = database_config("dummy_db", &config);

        // The size of `0` is an overflow wrap-around
        assert_eq!(Err(InvalidPoolSize(over_max)), database_config_result);
    }

    #[test]
    fn happy_path_database_config() {
        let url = "dummy_db.sqlite";
        let pool_size = 10;

        let mut database_extra = BTreeMap::new();
        let mut connection_config = BTreeMap::new();
        connection_config.insert("url".to_string(), Value::from(url));
        connection_config.insert("pool_size".to_string(), Value::from(pool_size));
        database_extra.insert("dummy_db", connection_config);

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config = database_config("dummy_db", &config).unwrap();

        assert_eq!(url, database_config.url);
        assert_eq!(pool_size, database_config.pool_size);
        assert_eq!(0, database_config.extras.len());
    }

    #[test]
    fn extras_do_not_contain_required_keys() {
        let url = "dummy_db.sqlite";
        let pool_size = 10;

        let mut database_extra = BTreeMap::new();
        let mut connection_config = BTreeMap::new();
        connection_config.insert("url".to_string(), Value::from(url));
        connection_config.insert("pool_size".to_string(), Value::from(pool_size));
        database_extra.insert("dummy_db", connection_config);

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config = database_config("dummy_db", &config).unwrap();

        assert_eq!(url, database_config.url);
        assert_eq!(pool_size, database_config.pool_size);
        assert_eq!(false, database_config.extras.contains_key("url"));
        assert_eq!(false, database_config.extras.contains_key("pool_size"));
    }

    #[test]
    fn extra_values_are_placed_in_extras_map() {
        let url = "dummy_db.sqlite";
        let pool_size = 10;
        let tls_cert = "certs.pem";
        let tls_key = "key.pem";

        let mut database_extra = BTreeMap::new();
        let mut connection_config = BTreeMap::new();
        connection_config.insert("url".to_string(), Value::from(url));
        connection_config.insert("pool_size".to_string(), Value::from(pool_size));
        connection_config.insert("certs".to_string(), Value::from(tls_cert));
        connection_config.insert("key".to_string(), Value::from(tls_key));
        database_extra.insert("dummy_db", connection_config);

        let config = Config::build(Environment::Development)
            .extra("databases", database_extra)
            .finalize()
            .unwrap();

        let database_config = database_config("dummy_db", &config).unwrap();

        assert_eq!(url, database_config.url);
        assert_eq!(pool_size, database_config.pool_size);
        assert_eq!(true, database_config.extras.contains_key("certs"));
        assert_eq!(true, database_config.extras.contains_key("key"));
    }
}

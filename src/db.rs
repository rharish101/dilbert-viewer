// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Utilities for working with the database
use std::time::Duration;

use async_trait::async_trait;
use deadpool_redis::{Config as RedisConfig, Connection, Pool, PoolError, Runtime};
use redis::{aio::ConnectionLike, AsyncCommands, RedisResult};
use serde::{de::DeserializeOwned, Serialize};

use crate::constants::{DB_TIMEOUT, MAX_DB_CONN};
use crate::errors::DbInitError;

/// Trait to get and set Redis key-values with automatic serde (de)serialization using JSON.
// `redis::RedisFuture` is basically a future returned by `async_trait`, so using the latter is
// basically free convenience.
#[async_trait]
pub trait SerdeAsyncCommands: AsyncCommands {
    /// Get a possibly-null value given a key.
    ///
    /// The null value indicates a missing key in the DB.
    async fn get<K, RV: DeserializeOwned>(&mut self, key: K) -> RedisResult<Option<RV>>
    where
        K: Serialize + Send + Sync,
    {
        let data: Option<Vec<u8>> = AsyncCommands::get(self, serde_json::to_vec(&key)?).await?;
        Ok(if let Some(data) = data {
            Some(serde_json::from_slice(data.as_slice())?)
        } else {
            None
        })
    }

    /// Set a value for a given key.
    async fn set<K, V>(&mut self, key: K, value: V) -> RedisResult<()>
    where
        K: Serialize + Send + Sync,
        V: Serialize + Send + Sync,
    {
        AsyncCommands::set::<_, _, ()>(
            self,
            serde_json::to_vec(&key)?,
            serde_json::to_vec(&value)?,
        )
        .await?;
        Ok(())
    }
}

// Auto-implement it where possible.
impl<T> SerdeAsyncCommands for T where T: AsyncCommands {}

/// Convenient trait for possibly-mocked Redis connection pools.
pub trait RedisPool {
    type ConnType: ConnectionLike + SerdeAsyncCommands;
    async fn get(&self) -> Result<Self::ConnType, PoolError>;
}

// Implement it for `deadpool-redis`.
impl RedisPool for Pool {
    type ConnType = Connection;
    async fn get(&self) -> Result<Self::ConnType, PoolError> {
        self.get().await
    }
}

/// Initialize the database connection pool for caching data.
///
/// # Arguments
/// * `url` - The URL used to connect to the database
pub fn get_db_pool(url: String) -> Result<deadpool_redis::Pool, DbInitError> {
    // Heroku needs SSL for its Redis addon, but uses a self-signed certificate. So simply disable
    // verification while keeping SSL.
    let config = RedisConfig::from_url(url + "#insecure");
    let pool_builder = config
        .builder()?
        .runtime(Runtime::Tokio1)
        .max_size(MAX_DB_CONN)
        .wait_timeout(Some(Duration::from_secs(DB_TIMEOUT)));
    Ok(pool_builder.build()?)
}

#[cfg(test)]
pub mod mock {
    use super::*;

    use deadpool::{
        managed::TimeoutType,
        unmanaged::{Object, Pool as UmPool, PoolError as UmPoolError},
    };
    use redis_test::MockRedisConnection;

    /// A pool for a mock Redis connection.
    pub type MockPool = UmPool<MockRedisConnection>;

    // Implement it for `redis-test`.
    impl RedisPool for MockPool {
        type ConnType = MockRedisConnection;
        async fn get(&self) -> Result<Self::ConnType, PoolError> {
            match self.get().await {
                Ok(conn) => Ok(Object::take(conn)),
                Err(UmPoolError::Timeout) => Err(PoolError::Timeout(TimeoutType::Wait)),
                Err(UmPoolError::Closed) => Err(PoolError::Closed),
                Err(UmPoolError::NoRuntimeSpecified) => Err(PoolError::NoRuntimeSpecified),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use actix_web::{rt::spawn, App, HttpServer};
    use portpicker::pick_unused_port;
    use rcgen::generate_simple_self_signed;
    use rustls::ServerConfig;
    use rustls_pki_types::PrivatePkcs8KeyDer;

    #[actix_web::test]
    /// Test the database connection pool initialization.
    ///
    /// This also tries to establish a connection to the database, since improper initialization
    /// can still succeed initially, while failing later.
    async fn test_database_pool_initialization() {
        let port = pick_unused_port().expect("No available port");
        let host = format!("localhost:{port}");

        // Generate self-signed certs for the mock server. Since Heroku also uses self-signed
        // certs, this is fine.
        let cert_key = generate_simple_self_signed(vec![host
            .split(':')
            .next()
            .expect("No port specified in mock server URI")
            .to_string()])
        .expect("Couldn't generate TLS certificates");
        let tls_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert_key.cert.der().to_owned()],
                PrivatePkcs8KeyDer::from(cert_key.key_pair.serialize_der()).into(),
            )
            .expect("Invalid TLS cert/key");

        // Start the mock TLS server on a single thread.
        let tls_server = HttpServer::new(App::new)
            .bind_rustls_0_23(host.clone(), tls_config)
            .expect("Couldn't bind mock server to host")
            .workers(1);
        let handle = spawn(tls_server.run());

        let pool = get_db_pool(format!("rediss://{host}")).expect("Couldn't initialize DB pool");
        // A connection isn't attempted unless one is requested from the pool. So do that, since
        // TLS setup errors aren't noticed during pool init.
        pool.get()
            .await
            .expect("Couldn't establish a connection to the DB");

        // Close the server.
        handle.abort();
    }
}

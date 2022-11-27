//! Utilities for working with the database.
// This file is part of Dilbert Viewer.
//
// Copyright (C) 2022  Harish Rajagopal <harish.rajagopals@gmail.com>
//
// Dilbert Viewer is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Dilbert Viewer is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Dilbert Viewer.  If not, see <https://www.gnu.org/licenses/>.
use std::env;
use std::time::Duration;

use async_trait::async_trait;
use deadpool_redis::{
    redis::{aio::ConnectionLike, AsyncCommands, RedisResult},
    Config as RedisConfig, Connection, Pool, PoolError, Runtime,
};
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
        AsyncCommands::set(self, serde_json::to_vec(&key)?, serde_json::to_vec(&value)?).await?;
        Ok(())
    }
}

// Auto-implement it where possible.
impl<T> SerdeAsyncCommands for T where T: AsyncCommands {}

/// Convenient trait for possibly-mocked Redis connection pools.
#[async_trait]
pub trait RedisPool {
    type ConnType: ConnectionLike + SerdeAsyncCommands;
    async fn get(&self) -> Result<Self::ConnType, PoolError>;
}

// Implement it for `deadpool-redis`.
#[async_trait]
impl RedisPool for Pool {
    type ConnType = Connection;
    async fn get(&self) -> Result<Self::ConnType, PoolError> {
        self.get().await
    }
}

/// Initialize the database connection pool for caching data.
pub async fn get_db_pool() -> Result<deadpool_redis::Pool, DbInitError> {
    // Heroku needs SSL for its Redis addon, but uses a self-signed certificate. So simply disable
    // verification while keeping SSL.
    let config = RedisConfig::from_url(env::var("REDIS_TLS_URL")? + "#insecure");
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
        managed::{PoolError as MPoolError, TimeoutType},
        unmanaged::{Object, Pool as UmPool, PoolError as UmPoolError},
    };
    use redis_test::MockRedisConnection;

    /// A pool for a mock Redis connection.
    pub type MockPool = UmPool<MockRedisConnection>;

    // Implement it for `redis-test`.
    #[async_trait]
    impl RedisPool for MockPool {
        type ConnType = MockRedisConnection;
        async fn get(&self) -> Result<Self::ConnType, PoolError> {
            match self.get().await {
                Ok(conn) => Ok(Object::take(conn)),
                Err(UmPoolError::Timeout) => Err(MPoolError::Timeout(TimeoutType::Wait)),
                Err(UmPoolError::Closed) => Err(MPoolError::Closed),
                Err(UmPoolError::NoRuntimeSpecified) => Err(MPoolError::NoRuntimeSpecified),
            }
        }
    }
}

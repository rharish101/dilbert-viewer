//! Utilities for the viewer app
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
use async_trait::async_trait;
use chrono::{format::ParseResult, NaiveDate, NaiveDateTime, Utc};
use deadpool_redis::redis::{AsyncCommands, RedisResult};
use serde::{de::DeserializeOwned, Serialize};

/// Return the current date.
///
/// The timezone is fixed to UTC so that the code is independent of local time.
pub fn curr_date() -> NaiveDate {
    Utc::now().date_naive()
}

/// Return the current datetime.
///
/// The timezone is fixed to UTC so that the code is independent of local time.
pub fn curr_datetime() -> NaiveDateTime {
    Utc::now().naive_utc()
}

/// Convert the date string (assumed in UTC) to a `chrono::NaiveDate` struct.
///
/// # Arguments
/// * `date` - The input date
/// * `fmt` - The format of the input date
pub fn str_to_date(date: &str, fmt: &str) -> ParseResult<NaiveDate> {
    NaiveDate::parse_from_str(date, fmt)
}

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

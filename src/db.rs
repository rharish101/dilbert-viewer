// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Utilities for working with the database
use std::time::Duration;

use chrono::NaiveDate;
use jiff::civil::Date;
use sea_orm::DbErr;
use sea_orm::{
    ConnectOptions, Database, DatabaseConnection, EntityTrait, Set, sea_query::OnConflict,
};

#[cfg(test)]
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use tracing::{debug, info};

use crate::constants::{DB_TIMEOUT, ENTITY_PREFIX, MAX_DB_CONN};
use crate::entities::comic;
use crate::scraper::ComicData;

/// Initialize a database connection pool from a URL.
///
/// # Arguments
/// * `url` - The URL used to connect to the database, e.g. `postgres://...` or
///   `sqlite::memory:` (the latter is used in tests)
pub(crate) async fn init_db(url: &str) -> Result<DatabaseConnection, DbErr> {
    let mut options = ConnectOptions::new(url);
    options
        .max_connections(MAX_DB_CONN)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(DB_TIMEOUT));
    Database::connect(options).await
}

/// Ensure the comics table exists, syncing the schema from the entity
/// definitions.
///
/// To be run at the start of each subcommand. SeaORM introspects the live
/// database and creates any missing tables/columns for the entities under
/// `ENTITY_PREFIX`.
pub(crate) async fn ensure_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.get_schema_registry(ENTITY_PREFIX).sync(db).await?;
    Ok(())
}

/// Get the comic data for a given date.
///
/// `None` is returned if there's no comic for that date, or if it hasn't been
/// scraped yet.
pub(crate) async fn get_comic(
    db: &DatabaseConnection,
    date: Date,
) -> Result<Option<ComicData>, DbErr> {
    let comic_data = comic::Entity::find_by_id(naive_date(date))
        .one(db)
        .await?
        .map(ComicData::from);
    if comic_data.is_some() {
        debug!("Retrieved data from DB: {comic_data:?}");
    } else {
        debug!("Missing data in DB for date: {date}")
    }
    Ok(comic_data)
}

/// Insert a comic for a given date, skipping if one already exists.
///
/// # Arguments
/// * `db` - The database connection
/// * `date` - The date of the comic being inserted
/// * `comic_data` - The comic data
/// * `overwrite` - Whether to overwrite data if a comic already exists
pub(crate) async fn insert_comic(
    db: &DatabaseConnection,
    date: Date,
    comic_data: ComicData,
    overwrite: bool,
) -> Result<(), DbErr> {
    debug!("Attempting to update database with: {comic_data:?}, overwrite: {overwrite}");
    let model = comic::ActiveModel::from((naive_date(date), comic_data));
    let mut on_conflict = OnConflict::column(comic::COLUMN.date);
    if overwrite {
        on_conflict
            .update_columns([comic::COLUMN.title])
            .update_columns([comic::COLUMN.img_url])
            .update_columns([comic::COLUMN.img_width])
            .update_columns([comic::COLUMN.img_height])
            .update_columns([comic::COLUMN.permalink]);
    } else {
        on_conflict.do_nothing();
    }
    comic::Entity::insert(model)
        .on_conflict(on_conflict)
        .try_insert()
        .exec(db)
        .await?;
    info!("Successfully stored data for {date} in database");
    Ok(())
}

/// Convert an entity into scraper comic data.
impl From<comic::Model> for ComicData {
    fn from(model: comic::Model) -> Self {
        Self {
            title: model.title,
            img_url: model.img_url,
            img_width: model.img_width,
            img_height: model.img_height,
            permalink: model.permalink,
        }
    }
}

/// Convert a Jiff date into a Chrono one for storage in the database.
///
/// SeaORM (via SQLx) only supports Chrono for SQL `DATE` columns, so Chrono
/// is confined to this conversion at the database boundary.
///
/// A Jiff date is always a valid Chrono date, so this cannot fail.
fn naive_date(date: Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year() as i32, date.month() as u32, date.day() as u32)
        .expect("A jiff date is always valid, but the conversion still failed")
}

/// Convert a date and comic data into an entity.
impl From<(NaiveDate, ComicData)> for comic::ActiveModel {
    fn from((date, data): (NaiveDate, ComicData)) -> Self {
        Self {
            date: Set(date),
            title: Set(data.title),
            img_url: Set(data.img_url),
            img_width: Set(data.img_width),
            img_height: Set(data.img_height),
            permalink: Set(data.permalink),
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use test_case::test_case;

    /// A test date (a weekday, so a real comic exists on this date).
    fn test_date() -> Date {
        Date::new(2000, 1, 15).unwrap()
    }

    /// Test comic data with the given title.
    fn test_comic_data(title: &str) -> ComicData {
        ComicData {
            title: String::from(title),
            img_url: "https://example.com/comic.png".into(),
            img_width: 580,
            img_height: 140,
            permalink: "https://dilbert.com/strip/2000-01-15".into(),
        }
    }

    /// An in-memory SQLite database with the comics schema.
    pub async fn test_db() -> DatabaseConnection {
        let mut options = ConnectOptions::new("sqlite::memory:");
        // `max_connections(1)` is required: each connection in the pool to `sqlite::memory:` is a
        // *separate* in-memory database, so more than one connection would see different (empty)
        // tables.
        options.max_connections(1);
        let db = Database::connect(options).await.unwrap();
        ensure_schema(&db).await.unwrap();
        db
    }

    #[actix_web::test]
    /// Test that `ensure_schema` creates the comics table.
    async fn test_ensure_schema_creates_table() {
        let db = test_db().await;
        let row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'comics'",
            ))
            .await
            .unwrap()
            .unwrap();
        let ddl: String = row.try_get("", "sql").unwrap();
        for column in [
            "date",
            "title",
            "img_url",
            "img_width",
            "img_height",
            "permalink",
        ] {
            assert!(ddl.contains(column), "Missing column {column} in: {ddl}");
        }
    }

    #[test_case(false; "missing comic")]
    #[test_case(true; "existing comic")]
    #[actix_web::test]
    /// Test that `get_comic` returns the stored comic, or `None` when missing.
    async fn test_get_comic(comic_exists: bool) {
        let db = test_db().await;
        let date = test_date();
        let expected = if comic_exists {
            let data = test_comic_data("A test comic");
            insert_comic(&db, date, data.clone(), false).await.unwrap();
            Some(data)
        } else {
            None
        };
        assert_eq!(get_comic(&db, date).await.unwrap(), expected);
    }

    #[test_case(None, false, "Updated"; "insert new")]
    #[test_case(None, true, "Updated"; "insert new with overwrite")]
    #[test_case(Some("Original"), false, "Original"; "skip existing")]
    #[test_case(Some("Original"), true, "Updated"; "overwrite existing")]
    #[actix_web::test]
    /// Test `insert_comic`, checking all fields are preserved and that
    /// overwriting (or not) existing entries works as expected.
    async fn test_insert_comic(existing: Option<&str>, overwrite: bool, expected_title: &str) {
        let db = test_db().await;
        let date = test_date();
        if let Some(title) = existing {
            insert_comic(&db, date, test_comic_data(title), false)
                .await
                .unwrap();
        }
        insert_comic(&db, date, test_comic_data("Updated"), overwrite)
            .await
            .unwrap();
        assert_eq!(
            get_comic(&db, date).await.unwrap(),
            Some(test_comic_data(expected_title))
        );
    }
}

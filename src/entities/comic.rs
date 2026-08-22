// SPDX-FileCopyrightText: 2026 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! SeaORM entity for the `comics` table
//!
//! One row per comic, keyed by date. Dates with no comic simply have no row.

use sea_orm::entity::prelude::*;

/// The comic for a given date.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "comics")]
pub struct Model {
    /// The date the comic was published
    #[sea_orm(primary_key, auto_increment = false)]
    pub date: chrono::NaiveDate,

    /// The title of the comic
    pub title: String,

    /// The URL to the comic image
    #[sea_orm(unique)]
    pub img_url: String,

    /// The width of the image
    pub img_width: i32,

    /// The height of the image
    pub img_height: i32,

    /// The permalink to the comic
    #[sea_orm(unique)]
    pub permalink: String,
}

impl ActiveModelBehavior for ActiveModel {}

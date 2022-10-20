--  Script to initialize required tables for the PostgreSQL cache
--  This file is part of Dilbert Viewer.
--
--  Copyright (C) 2022  Harish Rajagopal <harish.rajagopals@gmail.com>
--
--  Dilbert Viewer is free software: you can redistribute it and/or modify
--  it under the terms of the GNU Affero General Public License as published
--  by the Free Software Foundation, either version 3 of the License, or
--  (at your option) any later version.
--
--  Dilbert Viewer is distributed in the hope that it will be useful,
--  but WITHOUT ANY WARRANTY; without even the implied warranty of
--  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
--  GNU Affero General Public License for more details.
--
--  You should have received a copy of the GNU Affero General Public License
--  along with Dilbert Viewer.  If not, see <https://www.gnu.org/licenses/>.

-- Cache is used to avoid scraping every time
CREATE TABLE IF NOT EXISTS comic_cache (
	comic DATE NOT NULL, -- date of the comic
	img_url VARCHAR(255) NOT NULL, -- the comic image's URL
	title VARCHAR(255) NOT NULL, -- the title of the comic, if it exists (some comics don't have it)
	img_width INT NOT NULL, -- the comic image's width
	img_height INT NOT NULL, -- the comic image's height
	last_used TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, -- used for keeping only the most recent entries
	PRIMARY KEY (comic)
);

-- For efficient lookup of the oldest comic.
-- This is used for clearing the oldest comic, and enforcing a row limit.
CREATE INDEX IF NOT EXISTS idx_last_used ON comic_cache (last_used);


-- This will only have a single row for storing the latest date.
-- This single entry will be updated occasionally.
CREATE TABLE IF NOT EXISTS latest_date (
	latest DATE NOT NULL, -- the latest comic as of now
	last_check TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, -- when this entry was last updated
	PRIMARY KEY (latest)
);

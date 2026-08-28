<!--
SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>

SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Dilbert Viewer

[![Tests](https://img.shields.io/github/actions/workflow/status/rharish101/dilbert-viewer/tests.yml?branch=main&label=Tests&logo=github&style=flat-square)](https://github.com/rharish101/dilbert-viewer/actions/workflows/tests.yml)

A simple comic viewer for Dilbert by Scott Adams, hosted
[here](https://dilbert-viewer.rharish.dev).

It works by serving comic metadata from a PostgreSQL database. This metadata
**does not** include the comics themselves, rather they contain URLs to comics
hosted elsewhere. This metadata is scraped from the original Dilbert comics
website via the Wayback Machine as a one-time operation, then stored in the
database used by the web server.

## Running

First, build the project in release mode:

```sh
cargo build --release
```

Then, set up a PostgreSQL database. After that, run the scraper against the
database as follows:

```sh
DATABASE_URL=... cargo run populate
```

For more fine-grained control, see the exposed options for the scraper via
`cargo run populate --help`.

Once the database is fully populated, run the viewer locally as follows:

```sh
DATABASE_URL=... cargo run serve
```

### Environment Variables

For specifying the database details, set the `DATABASE_URL` environment variable
according to
[this specification](https://www.sea-ql.org/SeaORM/docs/install-and-config/connection/#postgres).
For example, to connect to the `dilbert` database in the PostgreSQL server at
localhost with username `myuser` and password `mypassword`, use:
`postgres://myuser:mypassword@localhost/dilbert`.

To set the log level for either mode (populate vs serve) of the viewer, set the
`RUST_LOG` environment variable according to
[this specification](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives).
For example, to view all logs at or above the `DEBUG` level, run:

```sh
RUST_LOG=debug DATABASE_URL=... cargo run ...
```

## Contributing

[pre-commit](https://pre-commit.com/) is used for managing hooks that run before
each commit (such as clippy), to ensure code quality. Thus, this needs to be set
up only when one intends to commit changes to git.

Firstly, [install pre-commit](https://pre-commit.com/#installation) itself.
Next, install pre-commit hooks:

```sh
pre-commit install
```

You can also use [prek](https://prek.j178.dev/) as a drop-in alternative to
pre-commit. Just replace all `pre-commit` invocations by `prek`.

One of the hooks runs [Prettier](https://prettier.io/) to format Markdown, JSON
and Jinja template files. This step requires **npm**, thus, install it along
with Node.js before running pre-commit.

pre-commit/prek install Prettier into their own isolated environments, so there
is no need to run `npm install` yourself, unless you want to run prettier
manually:

```sh
npm run format
```

For testing your changes using the provided test suite, run all tests as
follows:

```sh
cargo test
```

## Licenses

This repository uses [REUSE](https://reuse.software/) to document licenses. Each
file either has a header containing copyright and license information, or has an
entry in the [TOML file](https://reuse.software/spec-3.3/#reusetoml) at
[REUSE.toml](./REUSE.toml). The license files that are used in this project can
be found in the [LICENSES](./LICENSES) directory.

A copy of the AGPL-3.0-or-later license is placed in [LICENSE](./LICENSE), to
signify that it constitutes the majority of the codebase, and for compatibility
with GitHub.

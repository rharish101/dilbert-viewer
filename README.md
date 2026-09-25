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
DATABASE_URL=... dilbert-viewer populate
```

Once the database is fully populated, run the viewer locally as follows:

```sh
DATABASE_URL=... dilbert-viewer serve
```

## Command-Line Options

For more fine-grained control, see the full options list via:

```sh
dilbert-viewer --help
```

For subcommands (`serve`/`populate`):

```sh
dilbert-viewer <serve|populate> --help
```

Several options have environment variable fallbacks, e.g. `DATABASE_URL` for
`--database-url`, `PORT` for `--port`. `--help` should list the environment
variable for each such flag. A flag always takes precedence over its environment
variable.

### Notes

- The database URL format (`--database-url`) follows
  [this specification](https://www.sea-ql.org/SeaORM/docs/install-and-config/connection/#postgres).
  For example, `postgres://myuser:mypassword@localhost/dilbert`.
- `--log-level` uses the standard `RUST_LOG` environment variable with the
  [`tracing` filter syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives).
  For example, `debug`, or `dilbert_viewer=trace`.
- The default port, when one is not provided, is `5000`. If `5000` is already in
  use, a free port is randomly chosen.
- The `populate` subcommand's `date` argument is a repeatable positional
  argument; omit it to scrape every date.

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
with Node.js. Next, install Prettier and its plugins:

```sh
npm install
```

**NOTE:** pre-commit/prek depend on Prettier installed in 'node_modules'; they
don't install Prettier into their own isolated environments. Hence the above
step.

If you want to run Prettier manually:

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

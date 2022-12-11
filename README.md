# Dilbert Viewer
[![Tests](https://img.shields.io/github/workflow/status/rharish101/dilbert-viewer/Tests?label=Tests&logo=github&style=flat-square)](https://github.com/rharish101/dilbert-viewer/actions/workflows/tests.yml)

A simple comic viewer for Dilbert by Scott Adams, hosted on Render [here](https://dilbert-viewer.onrender.com).
It uses a [managed Redis instance](https://render.com/docs/redis) for caching.

## Setup
### Redis:
If you have a memory limit on your Redis database (like Render does), configure Redis to evict keys using the [`allkeys-lru` policy](https://redis.io/docs/reference/eviction/).

## Running
Build the project in release mode:
```sh
cargo build --release
```

Then, set the required environment variables and run the viewer locally:
```sh
REDIS_URL=... cargo run --release
```
Here, `REDIS_URL` is the URL to your Redis database, in the format described in the [redis-rs docs](https://docs.rs/redis/latest/redis/#connection-parameters).

If you want to run the viewer without a Redis database, then simply run it without the environment variable:
```sh
cargo run --release
```

## Contributing
[pre-commit](https://pre-commit.com/) is used for managing hooks that run before each commit (such as clippy), to ensure code quality.
Thus, this needs to be set up only when one intends to commit changes to git.

Firstly, [install pre-commit](https://pre-commit.com/#installation) itself.
Next, install pre-commit hooks:
```sh
pre-commit install
```

For testing your changes using the provided test suite, run all tests as follows:
```sh
cargo test
```

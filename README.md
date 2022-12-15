# Dilbert Viewer
[![Tests](https://img.shields.io/github/actions/workflow/status/rharish101/dilbert-viewer/tests.yml?branch=main&label=Tests&logo=github&style=flat-square)](https://github.com/rharish101/dilbert-viewer/actions/workflows/tests.yml)
[![Deployment](https://img.shields.io/github/deployments/rharish101/dilbert-viewer/dilbert-viewer?label=Deployment&logo=heroku&style=flat-square)](https://dilbert-viewer.herokuapp.com)

A simple comic viewer for Dilbert by Scott Adams, hosted on Heroku [here](https://dilbert-viewer.herokuapp.com).
It uses the third-party [Rust Buildpack](https://elements.heroku.com/buildpacks/emk/heroku-buildpack-rust) with the [Heroku Redis add-on](https://elements.heroku.com/addons/heroku-redis) for caching.

## Setup
The [Heroku CLI](https://devcenter.heroku.com/articles/heroku-cli) is used to locally run the code as specified in the [Procfile](./Procfile).
To install the Heroku CLI, please refer to [Heroku's installation guide](https://devcenter.heroku.com/articles/heroku-cli#download-and-install) for recommended installation options.

### Recommendation:
If you have a memory limit on your Redis database (like Heroku does), configure Redis to evict keys using the [`allkeys-lru` policy](https://redis.io/docs/reference/eviction/).
To configure this with Heroku's Redis addon, run the following:
```sh
heroku redis:maxmemory -a app-name --policy allkeys-lru
```
Here, `app-name` is the name of your Heroku app that has a Redis database configured.

## Running
Build the project in release mode:
```sh
cargo build --release
```

Then, set the required environment variables and run the viewer locally with the Heroku CLI:
```sh
REDIS_TLS_URL=$(heroku config:get REDIS_TLS_URL -a app-name) heroku local web
```
Here, `app-name` is the name of your Heroku app that has a Redis database configured.
You can also replace the value of this environment variable with a URL to your custom Redis database.

If you want to run the viewer without a Redis database, then simply run it without the environment variable:
```sh
heroku local web
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

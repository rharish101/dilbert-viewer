# Dilbert Viewer
[![Tests](https://img.shields.io/github/workflow/status/rharish101/dilbert-viewer/Tests?label=Tests&logo=github&style=flat-square)](https://github.com/rharish101/dilbert-viewer/actions/workflows/tests.yml)

A simple comic viewer for Dilbert by Scott Adams, hosted on Heroku [here](https://dilbert-viewer.herokuapp.com).
It uses the third-party [Rust Buildpack](https://elements.heroku.com/buildpacks/emk/heroku-buildpack-rust) with the [Heroku Redis add-on](https://elements.heroku.com/addons/heroku-redis) for caching.

## Local Setup
### Heroku
The [Heroku CLI](https://devcenter.heroku.com/articles/heroku-cli) is used to locally run the code as specified in the [Procfile](./Procfile).
To install the Heroku CLI, please refer to [Heroku's installation guide](https://devcenter.heroku.com/articles/heroku-cli#download-and-install) for recommended installation options.

#### Recommendation:
If you have a memory limit on your Redis database (like Heroku does), configure Redis to evict keys using the [`allkeys-lru` policy](https://redis.io/docs/reference/eviction/).
To configure this with Heroku's Redis addon, run the following:
```sh
heroku redis:maxmemory -a app-name --policy allkeys-lru
```
Here, `app-name` is the name of your Heroku app that has a Redis database configured.

### Running
1. Build the project in release mode:
    ```sh
    cargo build --release
    ```

2. Set the required environment variables and run the viewer locally with the Heroku CLI:
    ```sh
    REDIS_TLS_URL=$(heroku config:get REDIS_TLS_URL -a app-name) heroku local web
    ```
    Here, `app-name` is the name of your Heroku app that has a Redis database configured.
    
    If you want to run the viewer without a Redis database, then simply run it without the environment variable:
    ```sh
    heroku local web
    ```

### Testing
Run tests as follows:
```sh
cargo test
```

## Contributing
[pre-commit](https://pre-commit.com/) is used for managing hooks that run before each commit, to ensure code quality and run some basic tests.
Thus, this needs to be set up only when one intends to commit changes to git.

1. *[Optional]* Create a virtual environment for Python.

2. Install pre-commit, either globally or locally in the virtual environment:
    ```sh
    pip install pre-commit
    ```

3. Install pre-commit hooks:
    ```sh
    pre-commit install
    ```

**NOTE**: You need to be inside the virtual environment where you installed pre-commit every time you commit.
However, this is not required if you have installed it globally.

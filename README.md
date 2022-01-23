# Dilbert Viewer

A simple comic viewer for Dilbert by Scott Adams, hosted on Heroku [here](https://dilbert-viewer.herokuapp.com).
It uses the third-party [Rust Buildpack](https://elements.heroku.com/buildpacks/emk/heroku-buildpack-rust) with the [Heroku PostgreSQL add-on](https://elements.heroku.com/addons/heroku-postgresql) for caching.

## Instructions
Run the script `cache_init.sql` at the beginning to create the required tables in the cache:
```sh
heroku pg:psql -a dilbert-viewer -f cache_init.sql
```

### Local Testing
#### Setup
The [Heroku CLI](https://devcenter.heroku.com/articles/heroku-cli) is used to locally run the code as specified in the [Procfile](./Procfile).
To install the Heroku CLI, please refer to [Heroku's installation guide](https://devcenter.heroku.com/articles/heroku-cli#download-and-install) for recommended installation options.

#### Running
1. Build the project in release mode:
    ```sh
    cargo build --release
    ```

2. Set the required environment variables and run the viewer locally with the Heroku CLI:
    ```sh
    DATABASE_URL=$(heroku config:get DATABASE_URL -a dilbert-viewer) heroku local web
    ```

### For Contributing
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

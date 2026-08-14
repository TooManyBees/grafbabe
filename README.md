# grafbabe

The usecase: you have a service on your $7 VPS that has a Prometheus metrics endpoint. You want to monitor those stats over time, in lines on graphs. Grafana and Graphite exist, and they will OOM your poor, underresourced VPS as soon as you turn them on.

If all you want is lines on graphs, high five, because grafbabe will give you lines on graphs. I'm not saying this program is *good*, just that it's *good enough* for me, and maybe it's good enough for you.

## What it does

* Monitors one (and only one) Prometheus endpoint per process
* Collects one (and only one) month of metrics in a SQLite database
* Serves one (and only one) dashboard over HTTP
* Shows lines on graphs, that's it

## What it does not do

* Queries, insights, trends, alerts: these are all out of scope
* Terminate TLS for its dashboard page (use a reverse proxy for this)
* Hide the dashboard behind authentication (use a reverse proxy for this)
* Allow dynamic customization of the dashboard (but you can make your own, and manually compile them into your own binary)
* Deal with numbers big enough that f64s lose precision.
* Deal with any metric type except counters and gauges (histogram? I barely knew her!)

## Installation

Precompiled binaries with default features are attached to [each release](https://github.com/TooManyBees/grafbabe/releases). Alternatively, either `cargo install` or clone this repo and `cargo build --release`, choosing the features that you want (see [Cargo feature options](https://doc.rust-lang.org/cargo/reference/features.html#command-line-feature-options))).

Write yourself a config file (see [Configuration](#configuration)).

Consider a systemd unit file, or whatever task manager nonsense your server needs, to keep grafbabe a babe 24/7.

## Usage

`grafbabe` or `grafbabe serve` runs the server with default settings, which is not very useful unless the Prometheus endpoint you wish to monitor just so happens to be at `http://localhost:80/metrics`.

The option `-c /path/to/config.ini` reads settings from the file `/path/to/config.ini`. See [Configuration](#configuration) below for valid settings.

`grafbabe serve live` runs the server while serving frontend assets from the filesystem. (This is only useful when compiled for release with `--features serve_live`. In dev, this is identical to `grafbabe serve`.)

The option `-h` shows help.

The option `-v` shows the version.

The option `-vv` shows more detailed version, including how it was compiled.

Newer versions may try to upgrade your database in place, but they will back up the existing database before attempting to do so. `grafbabe -vv` will show the latest database revision, which you can compare to the query

```sql
select * from grafbabe_migrations;
```

## Configuration

You must use a config file in `ini` format in order to change grafbabe's settings. Use `grafbabe -c /path/to/config.ini` to choose the config file's location. The following is an example config file which also describes grafbabe's default behavior if a config file is not used.

```ini
# configuration for grafbabe


# Addresses to listen for HTTP requests
#
# This can one address, or multiple space-separated addresses
# if you want to listen on both an IPv4 and IPv6 address.
#
# If the addresses do not include a port, it defaults to 4242.
#
listen_addrs = 127.0.0.1:4242


# Address of Prometheus endpoint
#
# This is the source of the metrics to collect and visualize.
# grafbabe must be compiled with the `tls` feature in order to
# make requests over https.
#
prometheus_addr = http://localhost/metrics


# Directory of frontend assets
#
# If set, frontend assets will be served from this directory.
# In dev, the default is the "frontend" directory (included in
# the codebase). In release, this setting is ignored unless
# grafbabe is started with the command `serve live`.
#
frontend_dir = frontend


# Prometheus endpoint poll rate
#
# The duration to wait between polling `prometheus_addr` metrics.
#
# Valid values are a decimal number followed by `m` for minutes,
# `h` for hours, or `d` for days.
#
# Note: grafbabe is hardcoded to store a maximum of 1 month of
# metrics. Changing this value will affect the maximum size of
# the database under typical usage.
#
# Also note: if the poll rate is changed with existing metrics
# in the database, the metrics older than the change may appear
# compressed or expanded until they age out of the database.
# This is an intentional limitation.
#
poll_rate = 1m


# State location
#
# Directory on disk to persist state (namely the metrics database)
#
# If the location is omitted, it defaults to the current directory.
#
state_location = /var/lib/grafbabe


# Database name
#
# The database path is {state_location}/{database_name}.db3. If you
# leave this blank, it defaults to "grafbabe". Set this if you want
# multiple grafbabe processes storing their data in the same state
# location.
#
database_name = grafbabe


# Log level
#
# Options are ERROR, WARN, INFO, DEBUG, TRACE
#
log_level = INFO


# Log format
#
# Options are:
# - plain (single line log events, with no ANSI formatting)
# - pretty (optimized for readability, with ANSI formatting)
#
log_format = plain


# Log target
#
# Options are:
# - none (no logging)
# - stdout
# - stderr
# - systemd-journal (only when compiled with systemd_journal feature)
#
log_target = stderr

```

## Usage during development

Compiling with `--features mock_data` enables the following commands, which lets grafbabe work without a running Prometheus endpoint.

`grafbabe mock /path/to/mock.txt` reads fake data from `/path/to/mock.txt`, then runs the server using the mock data instead of reading from the database. The database is never modified in this case. The data should be in Prometheus format.

`grafbabe seed /path/to/mock.txt` reads fake data from `/path/to/mock.txt`, and inserts a single snapshot into the database using the current timestamp, then exits.

## Optional features

* **bundled_sqlite** (enabled by defualt) includes SQLite into the binary. You can disable this to link to system sqlite3 libraries, and shrink the binary.
* **color** (enabled by default) supports color ANSI output when log format is set to "pretty".
* **mock_data** as described in [Usage](#usage), enables the commands `grafbabe mock <path>` and `grafbabe seed <path>`.
* **serve_live** (enabled by default) as described in [Usage](#usage), enables the command `grafbabe serve live`.
* **systemd_journal** enables the log target "systemd-journal"
* **tls** allows grafbabe to make requests to a Prometheus endpoint over HTTPS.

Compile with `--no-default-features` to disable default features.

Compile with `--features <list,of,features>` to enable any.

`grafbabe -vv` will show which features were set during compilation.

## Frontend assets

When compiled for dev, grafbabe reads frontend HTML and JavaScript from the `frontend` directory, relative to the current working directory. The config file's `frontend_dir` value can be used to change this location. This setting is ignored in release.

When compiled in release, frontend assets are written into the binary. As in dev, the default is the `frontend` directory. To change this location, compile grafbabe with the env variable `GRAFBABE_FRONTEND` set to a different location.

When compiled in release with `--features serve_live`, the `serve live` command will serve frontend assets from the filesystem, as it does by default in dev. The config file's `frontend_dir` must be set. Only consider using this to test frontend changes before compiling them into a binary. **I'm not responsible for someone traversing your file tree while this is in use.**

Some notes on compiled frontend assets:

* The compiler will include files inside `GRAFBABE_FRONTEND`, but will not descend deeper into directories to find more.
* The compiler will ignore any dotfiles (files that begin with `.`).
* The compiler will ignore any JavaScript map files (files with a `.js.map` extension).

`grafbabe -vv` will show which files were included during compilation.

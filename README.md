# grafbabe

## Usage

`grafbabe` runs the server with default settings, which is not very useful unless the Prometheus endpoint you wish to monitor just so happens to be at `http://localhost:80/metrics`

`grafbabe -c /path/to/config.ini` runs the server with settings defined in `/path/to/config.ini`. See [Configuration](#configuration) below for valid options. All of the following commands are also influenced by this flag.

`grafbabe mock /path/to/mock.txt` reads fake data from `/path/to/mock.txt`, then runs the server using the mock data instead of reading from the database. The database is never modified in this case. The data should be in Prometheus format. *(This is only available when grafbabe is compiled with the **mock_data** feature.)*

`grafbabe seed /path/to/mock.txt` reads fake data from `/path/to/mock.txt`, and inserts a single snapshot into the database using the current timestamp, then exits. *(This is only available when grafbabe is compiled with the **mock_data** feature.)*

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
# grafbabe must be compiled with the `tls` feature (which is
# the default) in order to make requests over https.
#
prometheus_addr = http://localhost/metrics


# Prometheus endpoint poll rate
#
# The duration to wait between polling `prometheus_addr` metrics.
#
# Valid values are a decimal number followed by `m` for minutes, (
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
#
log_target = stderr

```
## Optional features

* **tls** (enabled by default) allows grafbabe to make requests to a Prometheus endpoint over HTTPS.
* **bundled_sqlite** (enabled by defualt) includes SQLite into the binary.
* **mock_data** as described in [Usage](#usage), it enables the commands `grafbabe mock <path>` and `grafbabe seed <path>`

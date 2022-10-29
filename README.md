# gantry-crane 🏗️

[![CI status](https://img.shields.io/github/workflow/status/philw07/gantry-crane/CI?label=CI&logo=github&style=flat-square)](https://github.com/philw07/gantry-crane/actions/workflows/ci.yml)
[![Docker status](https://img.shields.io/github/workflow/status/philw07/gantry-crane/Docker?label=Docker&logo=github&style=flat-square)](https://github.com/philw07/gantry-crane/actions/workflows/docker.yml)
[![Dependency status](https://deps.rs/repo/github/philw07/gantry-crane/status.svg?style=flat-square)](https://deps.rs/repo/github/philw07/gantry-crane)
[![Code coverage](https://img.shields.io/codecov/c/gh/philw07/gantry-crane?style=flat-square)](https://app.codecov.io/github/philw07/gantry-crane)
[![License](https://img.shields.io/github/license/philw07/gantry-crane?style=flat-square)](https://github.com/philw07/gantry-crane/blob/master/LICENSE)

A Docker to MQTT bridge which publishes information about your containers for other software to consume and allows to control them.  
Comes with a built-in [Home Assistant](https://www.home-assistant.io/) integration to monitor and control your containers right within Home Assistant.

## Getting started

It's recommended to run this app using docker.
Note that it's required to mount the docker socket into the container.

### Using docker

`docker run -d --name gantry-crane --restart=always -v /var/run/docker.sock:/var/run/docker.sock -e MQTT_HOST=localhost philw07/gantry-crane:latest`

or

`docker run -d --name gantry-crane --restart=always -v /var/run/docker.sock:/var/run/docker.sock -e MQTT_HOST=localhost ghcr.io/philw07/gantry-crane:latest`

### Using docker-compose

```
version: "3.5"

services:
  gantry-crane:
    image: philw07/gantry-crane:latest # or ghcr.io/philw07/gantry-crane:latest
    container_name: gantry-crane
    restart: always
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      - MQTT_HOST=localhost
```

## Configuration

The recommended way it to use environment variables for configuration.
Alternatively, a `gantry-crane.toml` file can be used (refer to [gantry-crane.example.toml](https://github.com/philw07/gantry-crane/blob/master/gantry-crane.example.toml)),
in which case the location of the file should be passed as argument `--config path/to/gantry-crane.toml`.

| Environment variable | Default value | Description |
| --- | --- | --- |
| `POLL_INTERVAL` | 60 | Interval in which all containers will be polled for their current status and updated on MQTT (in seconds). |
| `FILTER_BY_LABEL` | false | [Select which containers should be monitored](#select-containers-to-monitor). |
| `MQTT_HOST` | localhost | The host running the MQTT broker. |
| `MQTT_PORT` | 1883 | The port the MQTT broker listens to. |
| `MQTT_USERNAME` | None | MQTT username, only needed if the broker requires authentication. |
| `MQTT_PASSWORD` | None | MQTT password, only needed if the broker requires authentication. |
| `MQTT_TLS_ENCRYPTION` | false | Enable or disable tls encryption. |
| `MQTT_CA_CERTIFICATE` | None | Path to certificate authority file in PEM format. |
| `MQTT_CLIENT_CERTIFICATE` | None | Path to client certificate file in PEM format. |
| `MQTT_CLIENT_KEY` | None | Path to client key file in PEM format. |
| `MQTT_CLIENT_ID` | gantry-crane | MQTT client ID, should only be changed if two instances connect to the same broker. |
| `MQTT_BASE_TOPIC` | gantry-crane | Base topic under which all information is published. Should only be changed if two instances connect to the same broker. |
| `HOMEASSISTANT_ACTIVE` | false | Set to true to enable the Home Assistant integration. |
| `HOMEASSISTANT_BASE_TOPIC` | homeassistant | Must match the [discovery prefix set in Home Assistant](https://www.home-assistant.io/docs/mqtt/discovery/#discovery_prefix). |
| `HOMEASSISTANT_NODE_ID` | gantry-crane | The [node id](https://www.home-assistant.io/docs/mqtt/discovery/#discovery-topic) used for Home Assistant MQTT discovery. Should only be changed if two instances connect to the same broker. |

## Select containers to monitor

By default, gantry-crane bridges all containers to MQTT.
This might not be desirable, especially if you have a large amount of containers.

If you enable the `FILTER_BY_LABEL` setting, you can select which containers should be monitored by giving them the label `gantry-crane.enable=true`.
This can be done via [docker](https://docs.docker.com/engine/reference/commandline/run/#set-metadata-on-container--l---label---label-file) or [docker-compose](https://docs.docker.com/compose/compose-file/compose-file-v3/#labels).

## Home Assistant integration

When enabling the Home Assistant integration, by setting `HOMEASSISTANT_ACTIVE` to true, gantry-crane will publish MQTT discovery topics which Home Assistant will pick up automatically and add each container as a device with several entities (sensors and buttons).
Some entities are disabled by default and can be enabled via the Home Assistant UI.

The sensors and buttons can be used in automations or scenes, e.g. to start/stop containers at a specific time.

![Home Assistant integration example](https://github.com/philw07/gantry-crane/raw/master/docs/images/homeassistant_integration_example.png)

## Cleaning up

After stopping gantry-crane, the MQTT messages will be retained.
That means your container data will still be available on the MQTT broker and in Home Assistant if the integration is active.

To clean up all traces, you can run gantry-crane with the `--clean` flag and it will delete all retained messages.
Make sure to run it with the same configuration, otherwise it might not delete everything.

### Using docker

`docker run --rm -v /var/run/docker.sock:/var/run/docker.sock -e MQTT_HOST=localhost philw07/gantry-crane:latest --clean`

Make sure to omit `--restart=always`.

### Using docker-compose

```
version: "3.5"

services:
  gantry-crane:
    image: philw07/gantry-crane:latest
    container_name: gantry-crane
    # restart: always # Make sure to comment out or remove
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      - MQTT_HOST=localhost
    command: --clean
```

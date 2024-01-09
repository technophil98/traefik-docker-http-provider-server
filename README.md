# traefik-docker-http-provider-server

A Rust service generating a [Traefik Dynamic Config](https://doc.traefik.io/traefik/v2.10/providers/http/) from running Docker containers marked with
`traefik.http.*` labels.

This service was created to circumvent the [no-duplicate-providers Traefik limitation](https://github.com/traefik/traefik/issues/9101#issuecomment-1316970977).

## Configure it

### Required env variables

`traefik-docker-http-provider-server` requires the following env variable:

```dotenv
# The base url Traefik will use to route your services
# Watch out! The http:// prefix is required
BASE_URL=http://<my-host.local.domain>
```

You can create a `.env` file with the previous content or `export` them in your current shell.

#### Docker Labels

See [Routing Configuration with Labels](https://doc.traefik.io/traefik/v2.10/providers/docker/#routing-configuration-with-labels) from the Traefik & Docker section of Traefik's documentation.

## Run it

### Docker

```shell
docker run -p 8000:8000 --env-file .env ghcr.io/technophil98/traefik-docker-http-provider-server:latest
```

### Locally

```shell
# Export variables in .env to current shell
set -o allexport; source .env; set +o allexport
# Run it! Will be accessible at 'localhost:8000'
cargo run
```

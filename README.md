# Store Temporary Link

Make temporary links for storing stuff.

## Running

1. Install postgresql and start service

1. Install [shuttle](https://docs.shuttle.rs/introduction/installation)

1. Add a `Secrets.toml` file to add the redis url (you can use [upstash](https://upstash.com):

```
REDIS_URL="rediss://default:*@localhost:6379"
```

1. Run dev server:

```sh
cargo shuttle run
```

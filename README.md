# Send Temporary Link

Make temporary links for sending stuff.

## Running

1. Install postgresql and start service

2. Run schema

```sh
cp src/schema.sql /tmp
sudo -u postgres psql -d stlink -f /tmp/schema.sql
```

3. Install [shuttle](https://docs.shuttle.rs/introduction/installation)

4. Run dev server:

```sh
cargo shuttle run
```

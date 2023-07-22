# Send Temporary Link

Make temporary links for sending stuff.

## Running

1. Install postgresql

2. Run schema

```sql
cp src/schema.sql /tmp
sudo -u postgres psql -d stlink -f /tmp/schema.sql
```

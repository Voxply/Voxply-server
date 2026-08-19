-- Creates the farm's database next to the hub's on first PostgreSQL boot.
--
-- docker-entrypoint-initdb.d scripts run once, only when the data volume is
-- empty, and after POSTGRES_DB (the hub's `wavvon`) already exists. An
-- existing deployment adding a farm therefore needs this run by hand:
--   docker compose exec db psql -U wavvon -d wavvon -c 'CREATE DATABASE wavvon_farm OWNER wavvon;'
CREATE DATABASE wavvon_farm OWNER wavvon;

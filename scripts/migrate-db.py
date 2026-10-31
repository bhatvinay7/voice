#!/usr/bin/env python3
"""Apply versioned SQL migrations using psql and the configured PostgreSQL URL."""
import hashlib
import os
from pathlib import Path
import shlex
import subprocess
import sys
from urllib.parse import urlsplit, unquote, parse_qsl

ROOT = Path(__file__).resolve().parents[1]
env = os.environ.copy()
# Match backend precedence: process environment, service .env, repository .env.
for path in (ROOT / 'crates/services/server/.env', ROOT / '.env'):
    if path.exists():
        for line in path.read_text().splitlines():
            if not line.strip().removeprefix('export ').startswith(('POSTGRES_DIRECT_URL=', 'NEON_POOLER_URL=', 'POSTGRES_URL=', 'DATABASE_URL=')):
                continue
            parts = shlex.split(line, comments=True)
            if parts and parts[0] == 'export':
                parts = parts[1:]
            if len(parts) == 1 and '=' in parts[0]:
                key, value = parts[0].split('=', 1)
                env.setdefault(key, value)
url = env.get('POSTGRES_DIRECT_URL', env.get('NEON_POOLER_URL', env.get('POSTGRES_URL', env.get('DATABASE_URL', ''))))
if not url:
    sys.exit('Set POSTGRES_DIRECT_URL, NEON_POOLER_URL, POSTGRES_URL, or DATABASE_URL before running migrations.')
# Keep connection credentials out of process arguments.
connection = urlsplit(url)
if connection.scheme not in ('postgres', 'postgresql') or not connection.hostname:
    sys.exit('Expected a PostgreSQL connection URL.')
env['PGHOST'] = connection.hostname
env['PGPORT'] = str(connection.port or 5432)
env['PGUSER'] = unquote(connection.username or '')
env['PGPASSWORD'] = unquote(connection.password or '')
env['PGDATABASE'] = unquote(connection.path.lstrip('/'))
for key, value in parse_qsl(connection.query):
    supported = {'sslmode': 'PGSSLMODE', 'channel_binding': 'PGCHANNELBINDING',
                 'options': 'PGOPTIONS', 'connect_timeout': 'PGCONNECT_TIMEOUT'}
    if key not in supported:
        sys.exit(f'Unsupported connection option: {key}')
    env[supported[key]] = value
env['PGCONNECT_TIMEOUT'] = '15'

def psql(sql):
    result = subprocess.run(['psql', '-X', '-v', 'ON_ERROR_STOP=1'], input=sql,
                            text=True, env=env, capture_output=True)
    if result.returncode:
        print(result.stderr.replace(url, '[database URL]'), file=sys.stderr)
        sys.exit(result.returncode)
    print(result.stdout, end='')

if '--status' in sys.argv:
    psql("SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name;")
    sys.exit(0)

sql = "BEGIN;\nSELECT pg_advisory_xact_lock(72913024);\n"
sql += "CREATE TABLE IF NOT EXISTS public.schema_migrations (version text PRIMARY KEY, checksum text NOT NULL, applied_at timestamptz NOT NULL DEFAULT now());\n"
for path in sorted((ROOT / 'migrations').glob('*.sql')):
    version = path.name.replace("'", "''")
    content = path.read_text()
    checksum = hashlib.sha256(content.encode()).hexdigest()
    sql += f"DO $$ BEGIN IF EXISTS (SELECT 1 FROM public.schema_migrations WHERE version = '{version}' AND checksum <> '{checksum}') THEN RAISE EXCEPTION 'Migration checksum mismatch: {version}'; END IF; END $$;\n"
    sql += f"SELECT NOT EXISTS (SELECT 1 FROM public.schema_migrations WHERE version = '{version}') AS pending \\gset\n\\if :pending\n"
    sql += content + '\n'
    sql += f"INSERT INTO public.schema_migrations(version, checksum) VALUES ('{version}', '{checksum}');\n\\endif\n"
sql += 'COMMIT;\n'
psql(sql)

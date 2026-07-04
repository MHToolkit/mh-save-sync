CREATE TABLE IF NOT EXISTS accounts (
  account_handle BYTEA PRIMARY KEY,
  root_public_key BYTEA NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS devices (
  cert_id BYTEA PRIMARY KEY,
  account_handle BYTEA NOT NULL REFERENCES accounts(account_handle),
  device_public_key BYTEA NOT NULL,
  revoked_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS logical_saves (
  id TEXT PRIMARY KEY,
  account_handle BYTEA NOT NULL REFERENCES accounts(account_handle),
  encrypted_label BYTEA NOT NULL,
  head_snapshot_id TEXT,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS snapshots (
  id TEXT PRIMARY KEY,
  logical_save_id TEXT NOT NULL REFERENCES logical_saves(id),
  encrypted_manifest_object TEXT NOT NULL,
  committing_device_cert_id BYTEA NOT NULL REFERENCES devices(cert_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  conflict BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS snapshot_parents (
  snapshot_id TEXT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
  parent_snapshot_id TEXT NOT NULL,
  PRIMARY KEY (snapshot_id, parent_snapshot_id)
);

CREATE TABLE IF NOT EXISTS objects (
  object_id TEXT PRIMARY KEY,
  object_kind TEXT NOT NULL CHECK (object_kind IN ('chunk','manifest','export')),
  size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
  checksum_sha256 TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS upload_sessions (
  id UUID PRIMARY KEY,
  logical_save_id TEXT NOT NULL REFERENCES logical_saves(id),
  base_head TEXT,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS audit_events (
  id BIGSERIAL PRIMARY KEY,
  account_handle BYTEA,
  device_cert_id BYTEA,
  event_type TEXT NOT NULL,
  redacted JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

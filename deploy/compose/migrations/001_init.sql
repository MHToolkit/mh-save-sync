CREATE TABLE IF NOT EXISTS accounts (
  account_handle BYTEA PRIMARY KEY,
  root_public_key BYTEA NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS devices (
  cert_id BYTEA PRIMARY KEY,
  account_handle BYTEA NOT NULL REFERENCES accounts(account_handle),
  device_public_key BYTEA NOT NULL,
  certificate BYTEA NOT NULL,
  revoked_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS auth_challenges (
  id UUID PRIMARY KEY,
  account_handle BYTEA NOT NULL REFERENCES accounts(account_handle),
  device_cert_id BYTEA NOT NULL REFERENCES devices(cert_id),
  nonce BYTEA NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  used_at TIMESTAMPTZ,
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
  account_handle BYTEA NOT NULL REFERENCES accounts(account_handle),
  object_id TEXT NOT NULL,
  object_kind TEXT NOT NULL CHECK (object_kind IN ('chunk','manifest','export')),
  storage_key TEXT NOT NULL,
  size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
  checksum_sha256 TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (account_handle, object_id),
  UNIQUE (storage_key)
);

CREATE TABLE IF NOT EXISTS snapshot_objects (
  account_handle BYTEA NOT NULL,
  snapshot_id TEXT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
  object_id TEXT NOT NULL,
  PRIMARY KEY (snapshot_id, object_id),
  FOREIGN KEY (account_handle, object_id) REFERENCES objects(account_handle, object_id)
);

CREATE TABLE IF NOT EXISTS upload_sessions (
  id UUID PRIMARY KEY,
  account_handle BYTEA NOT NULL REFERENCES accounts(account_handle),
  device_cert_id BYTEA NOT NULL REFERENCES devices(cert_id),
  logical_save_id TEXT NOT NULL REFERENCES logical_saves(id),
  base_head TEXT,
  parents JSONB NOT NULL DEFAULT '[]'::jsonb,
  required_chunks JSONB NOT NULL DEFAULT '[]'::jsonb,
  manifest_id TEXT NOT NULL,
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

CREATE INDEX IF NOT EXISTS snapshots_logical_save_created_idx ON snapshots(logical_save_id, created_at DESC);
CREATE INDEX IF NOT EXISTS upload_sessions_expiry_idx ON upload_sessions(expires_at);
CREATE INDEX IF NOT EXISTS objects_created_idx ON objects(created_at);

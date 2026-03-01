CREATE TABLE IF NOT EXISTS repo (
    id BIGSERIAL PRIMARY KEY,
    owner TEXT NOT NULL,
    name TEXT NOT NULL,
    clone_url TEXT NOT NULL,
    default_image TEXT NOT NULL DEFAULT 'ubuntu:24.04',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(owner, name)
);

DO $$ BEGIN
    CREATE TYPE job_status AS ENUM ('queued', 'running', 'success', 'failed');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

CREATE TABLE IF NOT EXISTS job (
    id BIGSERIAL PRIMARY KEY,
    repo_id BIGINT NOT NULL REFERENCES repo(id),
    git_sha TEXT NOT NULL,
    git_ref TEXT NOT NULL,
    status job_status NOT NULL DEFAULT 'queued',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    claimed_by TEXT,
    claim_token UUID
);

CREATE TABLE IF NOT EXISTS job_log (
    id BIGSERIAL PRIMARY KEY,
    job_id BIGINT NOT NULL REFERENCES job(id),
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    line TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_job_status_created ON job(status, created_at) 
    WHERE status = 'queued';

CREATE INDEX IF NOT EXISTS idx_job_log_job_id ON job_log(job_id, ts);

DO $$ BEGIN
    CREATE TYPE stage_status AS ENUM ('pending', 'running', 'success', 'failed', 'skipped');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

CREATE TABLE IF NOT EXISTS job_stage (
    id BIGSERIAL PRIMARY KEY,
    job_id BIGINT NOT NULL REFERENCES job(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    status stage_status NOT NULL DEFAULT 'pending',
    stage_order INT NOT NULL DEFAULT 0,
    command TEXT,
    image TEXT,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    duration_ms BIGINT,
    exit_code INT,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(job_id, name)
);

CREATE TABLE IF NOT EXISTS stage_log (
    id BIGSERIAL PRIMARY KEY,
    stage_id BIGINT NOT NULL REFERENCES job_stage(id) ON DELETE CASCADE,
    line TEXT NOT NULL,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS job_metrics (
    id BIGSERIAL PRIMARY KEY,
    job_id BIGINT NOT NULL REFERENCES job(id) ON DELETE CASCADE,
    clone_duration_ms BIGINT,
    build_duration_ms BIGINT,
    test_duration_ms BIGINT,
    deploy_duration_ms BIGINT,
    total_duration_ms BIGINT,
    image_size_bytes BIGINT,
    cache_hit BOOLEAN DEFAULT FALSE,
    artifact_size_bytes BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(job_id)
);

CREATE TABLE IF NOT EXISTS scheduled_job (
    id BIGSERIAL PRIMARY KEY,
    repo_id BIGINT NOT NULL REFERENCES repo(id) ON DELETE CASCADE,
    cron_expression TEXT NOT NULL,
    branch TEXT NOT NULL DEFAULT 'main',
    timezone TEXT DEFAULT 'UTC',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_run_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(repo_id, branch)
);

ALTER TABLE job ADD COLUMN IF NOT EXISTS scheduled_job_id BIGINT REFERENCES scheduled_job(id);
ALTER TABLE job ADD COLUMN IF NOT EXISTS stages_json JSONB;
ALTER TABLE job ADD COLUMN IF NOT EXISTS metrics_json JSONB;

CREATE INDEX IF NOT EXISTS idx_job_stage_job_id ON job_stage(job_id);
CREATE INDEX IF NOT EXISTS idx_stage_log_stage_id ON stage_log(stage_id);
CREATE INDEX IF NOT EXISTS idx_scheduled_job_next_run ON scheduled_job(next_run_at) WHERE enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_job_metrics_job_id ON job_metrics(job_id);

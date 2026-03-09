CREATE TABLE IF NOT EXISTS runner (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    cpu INTEGER,
    memory_mb INTEGER,
    gpu INTEGER DEFAULT 0,
    arch TEXT NOT NULL DEFAULT 'x86_64',
    status TEXT NOT NULL DEFAULT 'offline' CHECK (status IN ('online', 'offline', 'busy')),
    last_heartbeat TIMESTAMPTZ,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_runner_name_unique ON runner(name);

ALTER TABLE job ADD COLUMN IF NOT EXISTS runner_id UUID REFERENCES runner(id) ON DELETE SET NULL;
ALTER TABLE job ADD COLUMN IF NOT EXISTS runner_requirements JSONB;

CREATE INDEX IF NOT EXISTS idx_runner_status ON runner(status);
CREATE INDEX IF NOT EXISTS idx_runner_tags ON runner USING GIN(tags);
CREATE INDEX IF NOT EXISTS idx_job_runner_id ON job(runner_id);

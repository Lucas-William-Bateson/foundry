-- Migration 003: Capture comprehensive GitHub data for future analysis
-- This adds many fields that may not be immediately used but will be valuable for analytics

-- ============================================
-- REPOSITORY TABLE ENHANCEMENTS
-- ============================================

-- GitHub repository metadata
ALTER TABLE repo ADD COLUMN IF NOT EXISTS github_id BIGINT;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS full_name TEXT;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS html_url TEXT;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS ssh_url TEXT;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS private BOOLEAN DEFAULT false;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS default_branch TEXT DEFAULT 'main';
ALTER TABLE repo ADD COLUMN IF NOT EXISTS language TEXT;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS topics TEXT[];
ALTER TABLE repo ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ;

-- ============================================
-- JOB TABLE ENHANCEMENTS  
-- ============================================

-- Push event metadata
ALTER TABLE job ADD COLUMN IF NOT EXISTS before_sha TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS compare_url TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS commits_count INTEGER;
ALTER TABLE job ADD COLUMN IF NOT EXISTS distinct_commits_count INTEGER;
ALTER TABLE job ADD COLUMN IF NOT EXISTS forced BOOLEAN DEFAULT false;
ALTER TABLE job ADD COLUMN IF NOT EXISTS deleted BOOLEAN DEFAULT false;
ALTER TABLE job ADD COLUMN IF NOT EXISTS created BOOLEAN DEFAULT false;

-- Pusher information
ALTER TABLE job ADD COLUMN IF NOT EXISTS pusher_name TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS pusher_email TEXT;

-- Sender (GitHub user who triggered the event)
ALTER TABLE job ADD COLUMN IF NOT EXISTS sender_id BIGINT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS sender_login TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS sender_avatar_url TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS sender_type TEXT;

-- Head commit extended info
ALTER TABLE job ADD COLUMN IF NOT EXISTS commit_author_email TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS commit_timestamp TIMESTAMPTZ;
ALTER TABLE job ADD COLUMN IF NOT EXISTS commit_tree_id TEXT;

-- Committer info (can be different from author)
ALTER TABLE job ADD COLUMN IF NOT EXISTS committer_name TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS committer_email TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS committer_username TEXT;

-- Commit stats (files changed)
ALTER TABLE job ADD COLUMN IF NOT EXISTS files_added TEXT[];
ALTER TABLE job ADD COLUMN IF NOT EXISTS files_modified TEXT[];
ALTER TABLE job ADD COLUMN IF NOT EXISTS files_removed TEXT[];

-- Installation info (for GitHub App)
ALTER TABLE job ADD COLUMN IF NOT EXISTS installation_id BIGINT;

-- GitHub Check Run info (for tracking)
ALTER TABLE job ADD COLUMN IF NOT EXISTS check_run_id BIGINT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS check_suite_id BIGINT;

-- Build metadata
ALTER TABLE job ADD COLUMN IF NOT EXISTS docker_image TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS exit_code INTEGER;
ALTER TABLE job ADD COLUMN IF NOT EXISTS error_message TEXT;

-- Raw payload storage for debugging/replay (optional, can be large)
-- ALTER TABLE job ADD COLUMN IF NOT EXISTS raw_payload JSONB;

-- Agent metadata
ALTER TABLE job ADD COLUMN IF NOT EXISTS agent_version TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS agent_hostname TEXT;

-- ============================================
-- NEW TABLES FOR COMPREHENSIVE TRACKING
-- ============================================

-- Store individual commits in a push (a push can have multiple commits)
CREATE TABLE IF NOT EXISTS job_commit (
    id BIGSERIAL PRIMARY KEY,
    job_id BIGINT NOT NULL REFERENCES job(id) ON DELETE CASCADE,
    sha TEXT NOT NULL,
    tree_id TEXT,
    message TEXT,
    author_name TEXT,
    author_email TEXT,
    author_username TEXT,
    committer_name TEXT,
    committer_email TEXT,
    committer_username TEXT,
    timestamp TIMESTAMPTZ,
    url TEXT,
    added TEXT[],
    modified TEXT[],
    removed TEXT[],
    distinct_commit BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(job_id, sha)
);

CREATE INDEX IF NOT EXISTS idx_job_commit_job_id ON job_commit(job_id);
CREATE INDEX IF NOT EXISTS idx_job_commit_sha ON job_commit(sha);

-- Store webhook events for debugging/replay
CREATE TABLE IF NOT EXISTS webhook_event (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    delivery_id TEXT,
    signature_valid BOOLEAN DEFAULT true,
    payload JSONB NOT NULL,
    processed BOOLEAN DEFAULT false,
    job_id BIGINT REFERENCES job(id),
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhook_event_type ON webhook_event(event_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_webhook_event_delivery ON webhook_event(delivery_id);

-- Build artifacts tracking (future use)
CREATE TABLE IF NOT EXISTS job_artifact (
    id BIGSERIAL PRIMARY KEY,
    job_id BIGINT NOT NULL REFERENCES job(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    size_bytes BIGINT,
    content_type TEXT,
    sha256 TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_job_artifact_job_id ON job_artifact(job_id);

-- Build metrics (for performance tracking)
CREATE TABLE IF NOT EXISTS job_metric (
    id BIGSERIAL PRIMARY KEY,
    job_id BIGINT NOT NULL REFERENCES job(id) ON DELETE CASCADE,
    metric_name TEXT NOT NULL,
    metric_value DOUBLE PRECISION NOT NULL,
    unit TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_job_metric_job_id ON job_metric(job_id);
CREATE INDEX IF NOT EXISTS idx_job_metric_name ON job_metric(metric_name, created_at DESC);

-- ============================================
-- ADDITIONAL INDEXES FOR ANALYTICS
-- ============================================

CREATE INDEX IF NOT EXISTS idx_job_sender_login ON job(sender_login);
CREATE INDEX IF NOT EXISTS idx_job_pusher_name ON job(pusher_name);
CREATE INDEX IF NOT EXISTS idx_job_git_ref ON job(git_ref);
CREATE INDEX IF NOT EXISTS idx_job_check_run ON job(check_run_id) WHERE check_run_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_repo_github_id ON repo(github_id) WHERE github_id IS NOT NULL;

-- Composite indexes for common queries
CREATE INDEX IF NOT EXISTS idx_job_repo_status_created ON job(repo_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_job_finished ON job(finished_at DESC) WHERE finished_at IS NOT NULL;

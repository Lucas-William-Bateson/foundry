-- Add more fields to repo for better tracking
ALTER TABLE repo ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS last_build_at TIMESTAMPTZ;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS build_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS success_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS failure_count INTEGER NOT NULL DEFAULT 0;

-- Add commit message and author to job
ALTER TABLE job ADD COLUMN IF NOT EXISTS commit_message TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS commit_author TEXT;
ALTER TABLE job ADD COLUMN IF NOT EXISTS commit_url TEXT;

-- Add cancelled status
DO $$ BEGIN
    ALTER TYPE job_status ADD VALUE IF NOT EXISTS 'cancelled';
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Better indexes
CREATE INDEX IF NOT EXISTS idx_job_repo_id ON job(repo_id);
CREATE INDEX IF NOT EXISTS idx_job_created_at ON job(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_repo_owner_name ON repo(owner, name);

-- Function to update repo stats on job completion
CREATE OR REPLACE FUNCTION update_repo_stats() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status IN ('success', 'failed') AND (OLD.status IS NULL OR OLD.status NOT IN ('success', 'failed')) THEN
        UPDATE repo SET
            last_build_at = NEW.finished_at,
            build_count = build_count + 1,
            success_count = success_count + CASE WHEN NEW.status = 'success' THEN 1 ELSE 0 END,
            failure_count = failure_count + CASE WHEN NEW.status = 'failed' THEN 1 ELSE 0 END
        WHERE id = NEW.repo_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trigger_update_repo_stats ON job;
CREATE TRIGGER trigger_update_repo_stats
    AFTER UPDATE ON job
    FOR EACH ROW
    EXECUTE FUNCTION update_repo_stats();

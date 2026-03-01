-- Store foundry.toml config in repo for webhook filtering
ALTER TABLE repo ADD COLUMN IF NOT EXISTS config_json JSONB;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS triggers_branches TEXT[] DEFAULT ARRAY['main', 'master'];
ALTER TABLE repo ADD COLUMN IF NOT EXISTS triggers_pull_requests BOOLEAN DEFAULT TRUE;
ALTER TABLE repo ADD COLUMN IF NOT EXISTS triggers_pr_target_branches TEXT[];

-- Index for schedule lookups
CREATE INDEX IF NOT EXISTS idx_repo_triggers ON repo(id) WHERE triggers_branches IS NOT NULL;

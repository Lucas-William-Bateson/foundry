-- Add 'scheduled' to the trigger_type enum for cron/scheduled jobs
DO $$ BEGIN
    ALTER TYPE trigger_type ADD VALUE IF NOT EXISTS 'scheduled';
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

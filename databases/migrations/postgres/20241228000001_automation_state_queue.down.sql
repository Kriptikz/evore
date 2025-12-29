DROP TRIGGER IF EXISTS trg_automation_queue_updated_at ON automation_state_queue;
DROP FUNCTION IF EXISTS update_automation_queue_updated_at();
DROP TABLE IF EXISTS automation_state_queue;
DROP TYPE IF EXISTS automation_queue_status;


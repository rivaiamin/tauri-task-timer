/** JIRA status id → human label. Keep in sync with apps/tui/src/jira.rs JIRA_STATUSES. */
export const STATUS_LABELS: Record<string, string> = {
	'11': 'To Do',
	'21': 'In Progress',
	'31': 'Done',
	'41': 'Local OK',
	'51': 'Cek di Local',
	'61': 'Cek di Prod',
	'71': 'KABARI SEKOLAH',
	'81': 'BLOCKED'
};

/** Tailwind classes for a status chip, by id or legacy snake_case value. */
const STATUS_COLORS: Record<string, string> = {
	'81': 'bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300',
	blocked: 'bg-red-100 dark:bg-red-900/40 text-red-700 dark:text-red-300',
	'21': 'bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300',
	in_progress: 'bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300',
	'31': 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300',
	'41': 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300',
	'51': 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300',
	'61': 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300',
	'71': 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300',
	done: 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300',
	completed: 'bg-green-100 dark:bg-green-900/40 text-green-700 dark:text-green-300'
};
const DEFAULT_CHIP_COLOR = 'bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400';

export function statusChipClass(status: string): string {
	return STATUS_COLORS[status] ?? DEFAULT_CHIP_COLOR;
}

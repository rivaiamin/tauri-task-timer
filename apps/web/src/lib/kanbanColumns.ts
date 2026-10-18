import { STATUS_LABELS } from '$lib/status';

/** Column order for the Kanban board (excludes Done). */
export const KANBAN_COLUMNS: string[] = [
	...Object.values(STATUS_LABELS).filter((label) => label !== 'Done'),
	'Other'
];

export function columnForJiraStatus(status: string): string {
	const trimmed = status.trim();
	if (!trimmed) return 'Other';
	for (const label of Object.values(STATUS_LABELS)) {
		if (label.toLowerCase() === trimmed.toLowerCase()) return label;
	}
	return 'Other';
}

export function isDoneJiraStatus(status: string): boolean {
	const t = status.trim();
	return t === '31' || t.toLowerCase() === 'done';
}

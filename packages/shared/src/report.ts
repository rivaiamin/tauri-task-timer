// Report builders shared by the web dashboard export, the /api/report endpoint,
// and (indirectly) the MCP get_report tool, so all three produce identical output.
import { secondsToStoryPoints } from './timer';

export interface ReportTask {
  label: string;
  description?: string | null;
  elapsedSeconds: number;
}

/**
 * Google-Chat-friendly "Daily Report": bold header + only tasks with recorded
 * time. Matches the desktop app's format.
 */
export function buildMarkdownReport(tasks: ReportTask[], dateISO: string): string {
  const lines = [`*Daily Report ${dateISO}*`];
  for (const t of tasks) {
    if (t.elapsedSeconds <= 0) continue;
    let line = `- [${secondsToStoryPoints(t.elapsedSeconds).toFixed(2)}] ${t.label}`;
    if (t.description && t.description.trim()) line += ` - ${t.description.trim()}`;
    lines.push(line);
  }
  return lines.join('\n');
}

/** CSV with all tasks: Task, Description, Story Points. */
export function buildCsvReport(tasks: ReportTask[]): string {
  const rows: string[][] = [['Task', 'Description', 'Story Points']];
  for (const t of tasks) {
    rows.push([t.label, t.description ?? '', secondsToStoryPoints(t.elapsedSeconds).toFixed(2)]);
  }
  return rows
    .map((row) => row.map((field) => `"${String(field ?? '').replace(/"/g, '""')}"`).join(','))
    .join('\r\n');
}

export const LANES = [
  'Intent',
  'Context',
  'Action',
  'State',
  'OTel',
  'Quality',
] as const;
export type Lane = (typeof LANES)[number];

export function laneForNodeKind(kind: string): Lane | null {
  switch (kind) {
    case 'user_message':            return 'Intent';
    case 'assistant_message':       return 'Context';
    case 'tool_call':               return 'Action';
    case 'tool_result':             return 'Action'; // merged into tool_call, but defensive
    case 'file_history_snapshot':   return 'State';
    case 'otel_span':               return 'OTel';
    default:                        return null;
  }
}

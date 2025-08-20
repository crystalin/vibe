export type LiveSegment = {
  id: string;
  start: number;
  stop: number;
  text: string;
  speaker?: string | null;
};

export type LiveStats = {
  segments?: number;
  latencyMs?: number;
  bufferSize?: number;
  chunksProcessed?: number;
};

export type LiveStatus = 'idle' | 'recording' | 'paused' | 'stopped' | 'error';

export type SpeakerMap = Record<string, { label: string; color: string }>;

export type LiveTranscriptionConfig = {
  model?: string;
  language?: string;
  minSpeechDuration?: number;
  minSilenceDuration?: number;
  vadSensitivity?: number;
  enableSpeakerDetection?: boolean;
};

const PALETTE = [
  '#3B82F6', '#10B981', '#F59E0B', '#EF4444',
  '#8B5CF6', '#06B6D4', '#84CC16', '#EC4899'
];

export const normalizeSpeaker = (s?: string | null): string | null => {
  return s?.trim() || null;
};

export const mkStableId = (sessionId: string, seg: { start: number; stop: number; text: string }): string => {
  return `${sessionId}:${seg.start}-${seg.stop}:${seg.text.length}`;
};

const hash = (s: string): number => {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h += (h << 1) + (h << 4) + (h << 7) + (h << 8) + (h << 24);
  }
  return Math.abs(h);
};

export const colorForSpeaker = (key: string): string => {
  return PALETTE[hash(key) % PALETTE.length];
};

export const labelForIndex = (idx: number): string => {
  return `S${idx + 1}`;
};

export const formatTime = (ms: number): string => {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  
  if (hours > 0) {
    return `${hours}:${String(minutes % 60).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
  }
  return `${minutes}:${String(seconds % 60).padStart(2, '0')}`;
};

export const exportSegmentsAsText = (segments: LiveSegment[], speakerMap: SpeakerMap, includeSpeakers: boolean = true): string => {
  const sortedSegments = [...segments].sort((a, b) => a.start - b.start);
  
  return sortedSegments.map(seg => {
    const speaker = seg.speaker && includeSpeakers ? speakerMap[seg.speaker]?.label : null;
    const prefix = speaker ? `[${speaker}] ` : '';
    return `${prefix}${seg.text}`;
  }).join('\n\n');
};

export const exportSegmentsAsSRT = (segments: LiveSegment[]): string => {
  const sortedSegments = [...segments].sort((a, b) => a.start - b.start);
  
  return sortedSegments.map((seg, idx) => {
    const startTime = formatSRTTime(seg.start);
    const endTime = formatSRTTime(seg.stop);
    return `${idx + 1}\n${startTime} --> ${endTime}\n${seg.text}\n`;
  }).join('\n');
};

const formatSRTTime = (ms: number): string => {
  const totalSeconds = ms / 1000;
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = Math.floor(totalSeconds % 60);
  const milliseconds = Math.floor((ms % 1000));
  
  return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')},${String(milliseconds).padStart(3, '0')}`;
};
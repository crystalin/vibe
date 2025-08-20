import { LiveSegment, SpeakerMap, formatTime } from '~/lib/live';

interface TranscriptSegmentProps {
  segment: LiveSegment;
  speakerMap: SpeakerMap;
  showTimestamp?: boolean;
}

export function TranscriptSegment({ segment, speakerMap, showTimestamp = false }: TranscriptSegmentProps) {
  const speaker = segment.speaker ? speakerMap[segment.speaker] : undefined;
  
  return (
    <div className="flex gap-2 py-1 px-2 hover:bg-base-200/50 rounded transition-colors">
      {showTimestamp && (
        <span className="text-xs text-base-content/50 font-mono min-w-[60px]">
          {formatTime(segment.start)}
        </span>
      )}
      {speaker && (
        <span 
          className="badge badge-sm text-white font-medium"
          style={{ 
            backgroundColor: speaker.color, 
            borderColor: speaker.color 
          }}
        >
          {speaker.label}
        </span>
      )}
      <span className="whitespace-pre-wrap flex-1">{segment.text}</span>
    </div>
  );
}
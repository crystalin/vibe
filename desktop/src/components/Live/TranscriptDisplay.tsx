import { useEffect, useRef } from 'react';
import { LiveSegment, SpeakerMap } from '~/lib/live';
import { TranscriptSegment } from './TranscriptSegment';

interface TranscriptDisplayProps {
  segments: LiveSegment[];
  speakerMap: SpeakerMap;
  autoScroll: boolean;
  showTimestamps?: boolean;
}

export function TranscriptDisplay({ 
  segments, 
  speakerMap, 
  autoScroll,
  showTimestamps = false 
}: TranscriptDisplayProps) {
  const tailRef = useRef<HTMLDivElement | null>(null);
  
  useEffect(() => {
    if (autoScroll && tailRef.current) {
      tailRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [segments.length, autoScroll]);
  
  return (
    <div className="overflow-auto h-full p-2">
      {segments.length === 0 ? (
        <div className="text-center text-base-content/50 py-8">
          <p className="text-lg mb-2">No transcript yet</p>
          <p className="text-sm">Click "Start" to begin live transcription</p>
        </div>
      ) : (
        <>
          {segments.map(segment => (
            <TranscriptSegment 
              key={segment.id} 
              segment={segment} 
              speakerMap={speakerMap}
              showTimestamp={showTimestamps}
            />
          ))}
          <div ref={tailRef} className="h-4" />
        </>
      )}
    </div>
  );
}
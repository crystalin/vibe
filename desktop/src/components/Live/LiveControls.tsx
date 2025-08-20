import { LiveStatus, LiveStats } from '~/lib/live';
import { FaPlay, FaPause, FaStop, FaTrash, FaDownload } from 'react-icons/fa';

interface LiveControlsProps {
  status: LiveStatus;
  stats?: LiveStats;
  autoScroll: boolean;
  isProcessing: boolean;
  onStart: () => void;
  onStop: () => void;
  onPause: () => void;
  onResume: () => void;
  onClear: () => void;
  onExport: (format: 'txt' | 'json' | 'srt') => void;
  setAutoScroll: (v: boolean) => void;
}

export function LiveControls({
  status,
  stats,
  autoScroll,
  isProcessing,
  onStart,
  onStop,
  onPause,
  onResume,
  onClear,
  onExport,
  setAutoScroll,
}: LiveControlsProps) {
  const isRecording = status === 'recording';
  const isPaused = status === 'paused';
  const isStopped = status === 'stopped';
  const isIdle = status === 'idle';
  const hasContent = isStopped || isRecording || isPaused;
  
  return (
    <div className="flex flex-col gap-3 p-3 border-b border-base-300">
      <div className="flex items-center gap-2">
        {(isIdle || isStopped) && (
          <button
            className="btn btn-primary gap-2"
            onClick={onStart}
            disabled={isProcessing}
          >
            <FaPlay className="text-sm" />
            Start Recording
          </button>
        )}
        
        {isRecording && (
          <button
            className="btn btn-warning gap-2"
            onClick={onPause}
            disabled={isProcessing}
          >
            <FaPause className="text-sm" />
            Pause
          </button>
        )}
        
        {isPaused && (
          <button
            className="btn btn-success gap-2"
            onClick={onResume}
            disabled={isProcessing}
          >
            <FaPlay className="text-sm" />
            Resume
          </button>
        )}
        
        {(isRecording || isPaused) && (
          <button
            className="btn btn-error gap-2"
            onClick={onStop}
            disabled={isProcessing}
          >
            <FaStop className="text-sm" />
            Stop
          </button>
        )}
        
        {hasContent && (
          <>
            <button
              className="btn btn-ghost gap-2"
              onClick={onClear}
              disabled={isProcessing || isRecording}
            >
              <FaTrash className="text-sm" />
              Clear
            </button>
            
            <div className="dropdown dropdown-bottom">
              <label 
                tabIndex={0} 
                className="btn btn-ghost gap-2"
              >
                <FaDownload className="text-sm" />
                Export
              </label>
              <ul 
                tabIndex={0} 
                className="dropdown-content menu p-2 shadow bg-base-100 rounded-box w-32 z-50"
              >
                <li><a onClick={() => onExport('txt')}>Text (.txt)</a></li>
                <li><a onClick={() => onExport('json')}>JSON (.json)</a></li>
                <li><a onClick={() => onExport('srt')}>SRT (.srt)</a></li>
              </ul>
            </div>
          </>
        )}
        
        <div className="flex-1" />
        
        <label className="label cursor-pointer gap-2">
          <span className="label-text">Auto-scroll</span>
          <input
            type="checkbox"
            className="toggle toggle-sm"
            checked={autoScroll}
            onChange={e => setAutoScroll(e.target.checked)}
          />
        </label>
      </div>
      
      <div className="flex items-center gap-4 text-sm">
        {isRecording && (
          <span className="badge badge-error badge-lg gap-1">
            <span className="loading loading-ring loading-xs"></span>
            Recording
          </span>
        )}
        
        {isPaused && (
          <span className="badge badge-warning badge-lg">
            Paused
          </span>
        )}
        
        {isStopped && (
          <span className="badge badge-neutral badge-lg">
            Stopped
          </span>
        )}
        
        {stats && (
          <>
            {typeof stats.segments === 'number' && (
              <span className="text-base-content/70">
                Segments: <span className="font-mono">{stats.segments}</span>
              </span>
            )}
            
            {typeof stats.latencyMs === 'number' && (
              <span className="text-base-content/70">
                Latency: <span className="font-mono">{Math.round(stats.latencyMs)}ms</span>
              </span>
            )}
            
            {typeof stats.bufferSize === 'number' && (
              <span className="text-base-content/70">
                Buffer: <span className="font-mono">{stats.bufferSize}</span>
              </span>
            )}
          </>
        )}
      </div>
    </div>
  );
}
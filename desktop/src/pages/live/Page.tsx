import { useLiveTranscription } from './viewModel';
import { LiveControls } from '~/components/Live/LiveControls';
import { TranscriptDisplay } from '~/components/Live/TranscriptDisplay';

export default function LivePage() {
  const vm = useLiveTranscription();
  
  return (
    <div className="flex flex-col h-full">
      <LiveControls
        status={vm.status}
        stats={vm.stats}
        autoScroll={vm.autoScroll}
        isProcessing={vm.isProcessing}
        onStart={vm.start}
        onStop={vm.stop}
        onPause={vm.pause}
        onResume={vm.resume}
        onClear={vm.clear}
        onExport={vm.exportTranscript}
        setAutoScroll={vm.setAutoScroll}
      />
      
      <div className="flex-1 overflow-hidden">
        <div className="h-full border border-base-300 rounded-lg m-3 mt-0 bg-base-100">
          <TranscriptDisplay
            segments={vm.segments}
            speakerMap={vm.speakerMap}
            autoScroll={vm.autoScroll}
            showTimestamps={true}
          />
        </div>
      </div>
      
      {vm.error && (
        <div className="alert alert-error mx-3 mb-3">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            className="stroke-current shrink-0 h-6 w-6"
            fill="none"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth="2"
              d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <span>{vm.error}</span>
        </div>
      )}
    </div>
  );
}
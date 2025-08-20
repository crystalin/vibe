import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import {
  LiveSegment,
  LiveStatus,
  LiveStats,
  LiveTranscriptionConfig,
  SpeakerMap,
  normalizeSpeaker,
  colorForSpeaker,
  labelForIndex,
  mkStableId,
} from '~/lib/live';
import { useToast } from '~/lib/hooks';
import { usePreferenceProvider } from '~/providers/Preference';

// Configuration constants
const AUDIO_CHUNK_INTERVAL_MS = 500; // Optimal for 50ms backend processing intervals
const MAX_AUDIO_QUEUE_SIZE = 100;
const AUDIO_QUEUE_WARNING_SIZE = 50;

type UseLiveReturn = {
  status: LiveStatus;
  sessionId?: string;
  segments: LiveSegment[];
  speakerMap: SpeakerMap;
  stats?: LiveStats;
  autoScroll: boolean;
  isProcessing: boolean;
  error?: string;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  clear: () => Promise<void>;
  exportTranscript: (format: 'txt' | 'json' | 'srt') => Promise<void>;
  setAutoScroll: (v: boolean) => void;
};

export function useLiveTranscription(): UseLiveReturn {
  const [status, setStatus] = useState<LiveStatus>('idle');
  const [sessionId, setSessionId] = useState<string>();
  const [segments, setSegments] = useState<LiveSegment[]>([]);
  const [speakerMap, setSpeakerMap] = useState<SpeakerMap>({});
  const [stats, setStats] = useState<LiveStats>();
  const [autoScroll, setAutoScroll] = useState(true);
  const [isProcessing, setIsProcessing] = useState(false);
  const [error, setError] = useState<string>();

  const toast = useToast();
  const { preferences } = usePreferenceProvider();

  const recRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const unsubsRef = useRef<UnlistenFn[]>([]);
  const seqRef = useRef(0);
  const warnedBackpressureRef = useRef(false);
  const pendingChunksRef = useRef<Blob[]>([]);
  const sendingRef = useRef(false);

  const pumpQueue = useCallback(async () => {
    if (sendingRef.current || !sessionId) return;
    sendingRef.current = true;
    
    try {
      while (pendingChunksRef.current.length > 0) {
        const blob = pendingChunksRef.current.shift()!;
        const buf = await blob.arrayBuffer();
        const bytes = Array.from(new Uint8Array(buf));
        
        await invoke('add_audio_to_live_transcription', {
          sessionId,
          audioData: bytes,
        });
        
        await new Promise(r => setTimeout(r, 0));
      }
    } catch (err) {
      console.error('Error sending audio:', err);
    } finally {
      sendingRef.current = false;
    }
  }, [sessionId]);

  const onData = useCallback((evt: BlobEvent) => {
    pendingChunksRef.current.push(evt.data);
    
    if (pendingChunksRef.current.length > AUDIO_QUEUE_WARNING_SIZE && !warnedBackpressureRef.current) {
      warnedBackpressureRef.current = true;
      toast('Audio is lagging; dropping old chunks to catch up', 'warning');
    }
    
    if (pendingChunksRef.current.length > MAX_AUDIO_QUEUE_SIZE) {
      pendingChunksRef.current.splice(0, pendingChunksRef.current.length - MAX_AUDIO_QUEUE_SIZE);
    }
    
    void pumpQueue();
  }, [pumpQueue, toast]);

  const setupEvents = useCallback(async (sid: string) => {
    const u1 = await listen('live_segment', (e: any) => {
      if (e?.payload?.sessionId !== sid) return;
      
      const seg = e.payload.segment as {
        start: number;
        stop: number;
        text: string;
        speaker?: string | null;
      };
      
      const speaker = normalizeSpeaker(seg.speaker);
      
      setSegments(prev => {
        const newSegmentId = mkStableId(sid, seg);
        const newSegment: LiveSegment = {
          id: newSegmentId,
          start: seg.start,
          stop: seg.stop,
          text: seg.text,
          speaker,
        };
        
        // Check if segment with same start time exists (could be an update)
        const existingIndex = prev.findIndex(p => p.start === newSegment.start);
        if (existingIndex !== -1) {
          // Update existing segment if text or other properties changed
          const existing = prev[existingIndex];
          if (existing.stop === newSegment.stop && existing.text === newSegment.text) {
            return prev; // No changes, skip update
          }
          const updatedSegments = [...prev];
          updatedSegments[existingIndex] = newSegment;
          return updatedSegments;
        }
        
        // Otherwise append new segment
        return [...prev, newSegment];
      });
      
      if (speaker) {
        setSpeakerMap(prev => {
          if (prev[speaker]) return prev;
          const idx = Object.keys(prev).length;
          return {
            ...prev,
            [speaker]: {
              label: labelForIndex(idx),
              color: colorForSpeaker(speaker),
            },
          };
        });
      }
    });

    const u2 = await listen('live_transcription_error', (e: any) => {
      if (e?.payload?.sessionId !== sid) return;
      setStatus('error');
      const message = e.payload?.message || 'Live transcription error';
      setError(message);
      toast(message, 'error');
    });

    const u3 = await listen('live_stats', (e: any) => {
      if (e?.payload?.sessionId !== sid) return;
      setStats(e.payload.stats as LiveStats);
    });

    unsubsRef.current = [u1, u2, u3];
  }, [toast]);

  const tearDownEvents = useCallback(() => {
    unsubsRef.current.forEach(u => u());
    unsubsRef.current = [];
  }, []);

  const setupRecorder = useCallback(async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      streamRef.current = stream;
      
      let mimeType = 'audio/webm';
      if (MediaRecorder.isTypeSupported('audio/webm;codecs=opus')) {
        mimeType = 'audio/webm;codecs=opus';
      }
      
      const rec = new MediaRecorder(stream, { mimeType });
      recRef.current = rec;
      rec.addEventListener('dataavailable', onData);
      rec.start(AUDIO_CHUNK_INTERVAL_MS);
      
      return true;
    } catch (err: any) {
      console.error('Error setting up recorder:', err);
      if (err.name === 'NotAllowedError' || err.name === 'PermissionDeniedError') {
        toast('Microphone permission denied. Please allow microphone access and try again.', 'error');
      } else if (err.name === 'NotFoundError') {
        toast('No microphone found. Please connect a microphone and try again.', 'error');
      } else {
        toast('Failed to access microphone: ' + err.message, 'error');
      }
      return false;
    }
  }, [onData, toast]);

  const tearDownRecorder = useCallback(() => {
    const rec = recRef.current;
    if (rec) {
      try {
        rec.stop();
      } catch {}
      rec.removeEventListener('dataavailable', onData);
    }
    recRef.current = null;
    
    const stream = streamRef.current;
    if (stream) {
      stream.getTracks().forEach(t => t.stop());
    }
    streamRef.current = null;
  }, [onData]);

  const start = useCallback(async () => {
    if (status === 'recording') return;
    
    setIsProcessing(true);
    setError(undefined);
    
    try {
      const config: LiveTranscriptionConfig = {
        model: preferences?.model,
        language: preferences?.language,
        minSpeechDuration: 250,
        minSilenceDuration: 500,
        vadSensitivity: 0.5,
        enableSpeakerDetection: false,
      };
      
      const sid = await invoke<string>('start_live_transcription', { config });
      setSessionId(sid);
      setStatus('recording');
      setSegments([]);
      setStats(undefined);
      setSpeakerMap({});
      seqRef.current = 0;
      warnedBackpressureRef.current = false;
      pendingChunksRef.current = [];
      
      await setupEvents(sid);
      const recordOk = await setupRecorder();
      
      if (!recordOk) {
        await invoke('stop_live_transcription', { sessionId: sid });
        setStatus('idle');
        setSessionId(undefined);
        tearDownEvents();
      }
    } catch (err: any) {
      console.error('Error starting transcription:', err);
      toast('Failed to start transcription: ' + err.message, 'error');
      setStatus('error');
      setError(err.message);
    } finally {
      setIsProcessing(false);
    }
  }, [status, preferences, setupEvents, setupRecorder, tearDownEvents, toast]);

  const stop = useCallback(async () => {
    if (!sessionId) return;
    
    setIsProcessing(true);
    
    try {
      await invoke('stop_live_transcription', { sessionId });
      tearDownEvents();
      tearDownRecorder();
      setStatus('stopped');
    } catch (err: any) {
      console.error('Error stopping transcription:', err);
      toast('Failed to stop transcription: ' + err.message, 'error');
    } finally {
      setIsProcessing(false);
    }
  }, [sessionId, tearDownEvents, tearDownRecorder, toast]);

  const pause = useCallback(async () => {
    if (!sessionId || status !== 'recording') return;
    
    setIsProcessing(true);
    try {
      await invoke('pause_live_transcription', { sessionId });
      recRef.current?.pause();
      setStatus('paused');
    } catch (err: any) {
      console.error('Error pausing transcription:', err);
      toast('Failed to pause transcription: ' + err.message, 'error');
    } finally {
      setIsProcessing(false);
    }
  }, [sessionId, status, toast]);

  const resume = useCallback(async () => {
    if (!sessionId || status !== 'paused') return;
    
    setIsProcessing(true);
    try {
      await invoke('resume_live_transcription', { sessionId });
      recRef.current?.resume();
      setStatus('recording');
    } catch (err: any) {
      console.error('Error resuming transcription:', err);
      toast('Failed to resume transcription: ' + err.message, 'error');
    } finally {
      setIsProcessing(false);
    }
  }, [sessionId, status, toast]);

  const clear = useCallback(async () => {
    if (!sessionId) return;
    
    try {
      await invoke('clear_live_transcription', { sessionId });
      setSegments([]);
      seqRef.current = 0;
    } catch (err: any) {
      console.error('Error clearing transcription:', err);
      toast('Failed to clear transcription: ' + err.message, 'error');
    }
  }, [sessionId, toast]);

  const exportTranscript = useCallback(async (format: 'txt' | 'json' | 'srt') => {
    if (!sessionId) return;
    
    try {
      await invoke('export_live_transcription', { sessionId, format });
      toast(`Transcript exported as ${format.toUpperCase()}`, 'success');
    } catch (err: any) {
      console.error('Error exporting transcription:', err);
      toast('Failed to export transcription: ' + err.message, 'error');
    }
  }, [sessionId, toast]);

  useEffect(() => {
    return () => {
      tearDownEvents();
      tearDownRecorder();
    };
  }, [tearDownEvents, tearDownRecorder]);

  return {
    status,
    sessionId,
    segments,
    speakerMap,
    stats,
    autoScroll,
    isProcessing,
    error,
    start,
    stop,
    pause,
    resume,
    clear,
    exportTranscript,
    setAutoScroll,
  };
}
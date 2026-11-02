import { useState, useEffect } from 'react';
import type { VoiceSettings, TtsVoice } from '../types';
import { listTtsVoices, updateVoiceSettings, speakText, stopSpeaking, prepareSttModel, onSttModelStatus, type SttModelStatus } from '../hooks/api';
import './Panel.css';
import './VoicePanel.css';

interface Props {
  settings: VoiceSettings;
  setSettings: (settings: VoiceSettings) => void;
  onClose: () => void;
}

/* The full voice settings UI (TTS + STT), reusable outside the panel
   wrapper - the consolidated Settings panel renders this as its Voice
   category. State handling is identical in both places. */
export function VoiceSettingsBody({ settings, setSettings }: Pick<Props, 'settings' | 'setSettings'>) {
  const [voices, setVoices] = useState<TtsVoice[]>([]);
  const [isTesting, setIsTesting] = useState(false);
  const [sttStatus, setSttStatus] = useState<SttModelStatus | null>(null);

  useEffect(() => {
    listTtsVoices().then(setVoices).catch(() => {});
    const unsub = onSttModelStatus(setSttStatus);
    return () => { unsub.then(fn => fn()); };
  }, []);

  // Switch STT model: persist, then prepare (download if needed) with status.
  const handleSttModel = (value: string) => {
    handleChange('stt_model', value);
    setSttStatus({ state: 'downloading', model: value });
    prepareSttModel().catch(() => {});
  };

  const handleChange = (field: keyof VoiceSettings, value: string | number | boolean) => {
    const updated = { ...settings, [field]: value };
    setSettings(updated);
    updateVoiceSettings(updated).catch(() => {});
  };

  const handleTestVoice = async () => {
    if (isTesting) {
      await stopSpeaking();
      setIsTesting(false);
      return;
    }
    setIsTesting(true);
    try {
      await speakText('Hello, I am your Orbix Core assistant.');
    } catch {
      // ignore
    }
    setIsTesting(false);
  };

  // Group voices: Kokoro (neural, featured) first, then system voices
  const kokoroFemale = voices.filter(v => v.provider === 'kokoro' && v.gender === 'female');
  const kokoroMale = voices.filter(v => v.provider === 'kokoro' && v.gender === 'male');
  const systemEnglish = voices.filter(v => v.provider === 'system' && v.lang?.startsWith('en'));
  const systemOther = voices.filter(v => v.provider === 'system' && !v.lang?.startsWith('en'));
  const currentVoice = voices.find(v => v.name === settings.tts_voice);

  return (
    <>
        {/* TTS Section */}
        <div className="voice-section">
          <h3 className="voice-section-title">Text-to-Speech</h3>

          <label className="form-label" htmlFor="voice-tts-voice">Voice</label>
          <select
            id="voice-tts-voice"
            className="form-input form-select"
            value={settings.tts_voice}
            onChange={e => handleChange('tts_voice', e.target.value)}
          >
            {kokoroFemale.length > 0 && (
              <optgroup label="✨ Neural - Female (Kokoro)">
                {kokoroFemale.map(v => (
                  <option key={v.name} value={v.name}>{v.display_name}</option>
                ))}
              </optgroup>
            )}
            {kokoroMale.length > 0 && (
              <optgroup label="✨ Neural - Male (Kokoro)">
                {kokoroMale.map(v => (
                  <option key={v.name} value={v.name}>{v.display_name}</option>
                ))}
              </optgroup>
            )}
            {systemEnglish.length > 0 && (
              <optgroup label="System - English">
                {systemEnglish.map(v => (
                  <option key={v.name} value={v.name}>{v.display_name}</option>
                ))}
              </optgroup>
            )}
            {systemOther.length > 0 && (
              <optgroup label="System - Other Languages">
                {systemOther.map(v => (
                  <option key={v.name} value={v.name}>{v.display_name}</option>
                ))}
              </optgroup>
            )}
          </select>
          {currentVoice?.provider === 'kokoro' && (
            <div className="voice-hint" style={{ marginTop: 6, marginBottom: 8 }}>
              <span style={{ color: 'var(--accent)', fontWeight: 600 }}>Neural voice</span> - first use downloads a ~340MB model. Runs locally on CPU.
            </div>
          )}

          <label className="form-label" htmlFor="voice-tts-rate">
            Speed: {settings.tts_rate} wpm
          </label>
          <input
            id="voice-tts-rate"
            type="range"
            className="voice-slider"
            min={80}
            max={300}
            step={5}
            value={settings.tts_rate}
            onChange={e => handleChange('tts_rate', Number(e.target.value))}
          />

          <div className="voice-row">
            <label className="form-label">Auto-speak responses</label>
            <button
              className={`toggle-switch ${settings.auto_speak ? 'on' : ''}`} role="switch" aria-checked={settings.auto_speak} aria-label="Auto-speak responses"
              onClick={() => handleChange('auto_speak', !settings.auto_speak)}
            >
              <span className="toggle-thumb" />
            </button>
          </div>

          <button
            className={`voice-test-btn ${isTesting ? 'active' : ''}`}
            onClick={handleTestVoice}
          >
            {isTesting ? (
              <>
                <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor">
                  <rect x="6" y="6" width="12" height="12" rx="2"/>
                </svg>
                Stop
              </>
            ) : (
              <>
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <polygon points="5 3 19 12 5 21 5 3"/>
                </svg>
                Test Voice
              </>
            )}
          </button>
        </div>

        {/* STT Section */}
        <div className="voice-section">
          <h3 className="voice-section-title">Speech-to-Text</h3>

          <label className="form-label" htmlFor="voice-stt-language">Language</label>
          <select
            id="voice-stt-language"
            className="form-input form-select"
            value={settings.stt_language}
            onChange={e => handleChange('stt_language', e.target.value)}
          >
            <option value="en-US">English (US)</option>
            <option value="en-GB">English (UK)</option>
            <option value="es-ES">Spanish</option>
            <option value="fr-FR">French</option>
            <option value="de-DE">German</option>
            <option value="it-IT">Italian</option>
            <option value="pt-BR">Portuguese (BR)</option>
            <option value="ja-JP">Japanese</option>
            <option value="zh-CN">Chinese (Simplified)</option>
            <option value="ko-KR">Korean</option>
          </select>

          <label className="form-label" htmlFor="voice-stt-model">Transcription model</label>
          <select
            id="voice-stt-model"
            className="form-input form-select"
            value={settings.stt_model ?? 'base'}
            onChange={e => handleSttModel(e.target.value)}
          >
            <option value="tiny">Fast (Tiny, ~78MB): quickest, lower accuracy</option>
            <option value="base">Balanced (Base, ~148MB): recommended</option>
            <option value="small">Accurate (Small, ~488MB): best English, slower</option>
            <option value="turbo">Best (Turbo, ~1.6GB): multilingual, near-flawless</option>
          </select>
          {sttStatus && sttStatus.state !== 'ready' && (
            <div className="voice-hint" style={{ marginTop: 6, color: sttStatus.state === 'error' ? 'var(--error)' : 'var(--accent)' }}>
              {sttStatus.state === 'downloading' ? 'Downloading transcription model...' : `Failed: ${sttStatus.error ?? 'download error'}`}
            </div>
          )}
          <div className="voice-hint" style={{ marginTop: 6, marginBottom: 10 }}>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>
            </svg>
            Changing this downloads the new model on first use.
          </div>

          <div className="voice-hint">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="12" cy="12" r="10"/>
              <path d="M12 16v-4"/>
              <path d="M12 8h.01"/>
            </svg>
            Click the mic button in the chat input to start recording. Click again to stop and transcribe.
          </div>
        </div>
    </>
  );
}

export default function VoicePanel({ settings, setSettings, onClose }: Props) {
  return (
    <div className="panel voice-panel">
      <div className="panel-header">
        <h2 className="panel-title">Voice</h2>
        <button className="panel-close-btn" onClick={onClose} title="Close" aria-label="Close">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      <div className="panel-body">
        <VoiceSettingsBody settings={settings} setSettings={setSettings} />
      </div>
    </div>
  );
}

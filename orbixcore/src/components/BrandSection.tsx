import { useState, useEffect, useCallback } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import {
  listBrandKits, setActiveBrandKit, updateBrandKit, deleteBrandKit,
  extractBrandKitFromFile, createBrandedDocument, getBrandLogoData,
  type BrandKit,
} from '../hooks/api';

/* Brand kit manager (Settings > Brand). Extract a kit from a guideline
   file, review/edit it, and everything Bob generates (Word, PowerPoint,
   HTML) carries the brand. */
export default function BrandSection() {
  const [kits, setKits] = useState<BrandKit[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [extracting, setExtracting] = useState(false);
  const [error, setError] = useState('');
  const [sampleBusy, setSampleBusy] = useState<string | null>(null);
  const [logoData, setLogoData] = useState<{ mime: string; base64: string } | null>(null);

  const refresh = useCallback(() => {
    listBrandKits().then(r => { setKits(r.kits); setActiveId(r.active_id); }).catch(() => {});
    getBrandLogoData().then(setLogoData).catch(() => {});
  }, []);
  useEffect(() => { refresh(); }, [refresh]);

  const active = kits.find(k => k.id === activeId) ?? kits[0] ?? null;

  const handleExtract = async () => {
    setError('');
    const picked = await openDialog({
      multiple: false,
      title: 'Choose your brand guideline',
      filters: [{ name: 'Guideline', extensions: ['pdf', 'md', 'txt', 'html', 'htm'] }],
    }).catch(() => null);
    if (typeof picked !== 'string') return;
    setExtracting(true);
    try {
      await extractBrandKitFromFile(picked);
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setExtracting(false);
    }
  };

  const handleSample = async (kind: string) => {
    if (!active) return;
    setSampleBusy(kind);
    try {
      await createBrandedDocument(kind, {
        title: `${active.name} Brand Sample`,
        subtitle: 'How your documents will look',
        sections: [
          {
            heading: 'Your brand, applied',
            body: 'Every document Bob creates uses these colors, fonts, and this tone of voice.',
            bullets: active.colors.slice(0, 4).map(c => `${c.name}: ${c.hex} (${c.role})`),
          },
          {
            heading: 'Tone of voice',
            body: active.tone,
            bullets: [],
          },
        ],
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setSampleBusy(null);
    }
  };

  const handlePickLogo = async () => {
    const picked = await openDialog({
      multiple: false,
      title: 'Choose your logo (PNG or JPEG)',
      filters: [{ name: 'Logo', extensions: ['png', 'jpg', 'jpeg'] }],
    }).catch(() => null);
    if (typeof picked !== 'string' || !active) return;
    const updated = { ...active, logo_path: picked };
    setKits(prev => prev.map(k => k.id === updated.id ? updated : k));
    await updateBrandKit(updated).catch(() => {});
    getBrandLogoData().then(setLogoData).catch(() => {});
  };

  const patchActive = (patch: Partial<BrandKit>) => {
    if (!active) return;
    const updated = { ...active, ...patch };
    setKits(prev => prev.map(k => k.id === updated.id ? updated : k));
    updateBrandKit(updated).catch(() => {});
  };

  return (
    <>
      <div className="panel-section-header">Brand Kit</div>
      <p className="brand-intro">
        Drop your brand guideline into a confidential folder, extract it here, and every
        document Bob creates (Word, PowerPoint, HTML) carries your design. Extraction runs
        fully on this machine.
      </p>

      <button className="panel-btn accent full" onClick={handleExtract} disabled={extracting}>
        {extracting ? 'Reading your guideline locally...' : 'Extract kit from guideline file...'}
      </button>
      {error && <div className="form-error">{error}</div>}

      {active && (
        <>
          <div className="panel-section-header" style={{ marginTop: 20 }}>
            Active kit: {active.name}
          </div>
          <div className="brand-swatches">
            {active.colors.map((c, i) => (
              <div key={i} className="brand-swatch" title={`${c.name} (${c.role})`}>
                <span className="brand-swatch-chip" style={{ background: c.hex }} />
                <span className="brand-swatch-hex">{c.hex}</span>
                <span className="brand-swatch-role">{c.role}</span>
              </div>
            ))}
          </div>

          <div className="brand-logo-row">
            {logoData ? (
              <img
                className="brand-logo-img"
                alt="Brand logo"
                src={`data:${logoData.mime};base64,${logoData.base64}`}
              />
            ) : (
              <span className="brand-logo-missing">
                No logo yet. Guidelines rarely embed one as text - pick the file directly.
              </span>
            )}
            <button className="panel-btn" onClick={handlePickLogo}>
              {logoData ? 'Change logo...' : 'Add logo...'}
            </button>
          </div>

          <div className="brand-field-row">
            <label className="form-label">Heading font</label>
            <input
              className="form-input"
              value={active.heading_font}
              onChange={e => patchActive({ heading_font: e.target.value })}
            />
          </div>
          <div className="brand-field-row">
            <label className="form-label">Body font</label>
            <input
              className="form-input"
              value={active.body_font}
              onChange={e => patchActive({ body_font: e.target.value })}
            />
          </div>
          <label className="form-label" style={{ marginTop: 8 }}>Tone of voice</label>
          <textarea
            className="form-input"
            rows={3}
            value={active.tone}
            onChange={e => patchActive({ tone: e.target.value })}
          />

          <div className="panel-section-header" style={{ marginTop: 20 }}>Preview the brand</div>
          <div className="brand-sample-row">
            {[['docx', 'Word'], ['pptx', 'PowerPoint'], ['html', 'Web page']].map(([kind, label]) => (
              <button
                key={kind}
                className="panel-btn"
                disabled={sampleBusy !== null}
                onClick={() => handleSample(kind)}
              >
                {sampleBusy === kind ? 'Generating...' : `Sample ${label}`}
              </button>
            ))}
          </div>
          <span className="brand-hint">
            Samples land in Documents / Orbix Documents and open automatically. In chat, just ask:
            "create a PowerPoint about Q3 results".
          </span>
        </>
      )}

      {kits.length > 1 && (
        <>
          <div className="panel-section-header" style={{ marginTop: 20 }}>All kits</div>
          {kits.map(k => (
            <div key={k.id} className="brand-kit-row">
              <span className="brand-kit-dot" style={{ background: k.colors[0]?.hex ?? '#888' }} />
              <span className="brand-kit-name">{k.name}</span>
              {k.id === activeId ? (
                <span className="brand-kit-active">active</span>
              ) : (
                <button className="link-btn" onClick={() => setActiveBrandKit(k.id).then(refresh)}>Use</button>
              )}
              <button
                className="link-btn danger"
                onClick={() => deleteBrandKit(k.id).then(refresh)}
              >
                Remove
              </button>
            </div>
          ))}
        </>
      )}
    </>
  );
}

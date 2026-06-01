/**
 * AI tab.
 *
 * Holds the local Ollama endpoint, keep-warm controls, and the custom system
 * prompt. The active model picker lives in the main app overlay (see
 * ModelPickerPanel) since model selection is runtime UI state owned by
 * ActiveModelState in the backend, not a TOML-persisted field. The
 * Window/Quote knobs live in the Display tab.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import {
  Section,
  Dropdown,
  NumberSlider,
  NumberStepper,
  TextField,
  Textarea,
  Toggle,
} from '../components';
import { SaveField } from '../components/SaveField';
import { useDebouncedSave } from '../hooks/useDebouncedSave';
import { configHelp } from '../configHelpers';
import { DrawCheckIcon } from '../../components/DrawCheckIcon';
import { Tooltip } from '../../components/Tooltip';
import styles from '../../styles/settings.module.css';
import type { RawAppConfig } from '../types';

interface ModelTabProps {
  config: RawAppConfig;
  resyncToken: number;
  onSaved: (next: RawAppConfig) => void;
}

type Provider = 'ollama' | 'openrouter';
type VoiceProvider = 'supertonic' | 'openrouter';

const PROVIDER_OPTIONS: readonly Provider[] = ['ollama', 'openrouter'];
const VOICE_PROVIDER_OPTIONS: readonly VoiceProvider[] = [
  'supertonic',
  'openrouter',
];

const DEFAULT_OPENROUTER = {
  api_key: '',
  base_url: 'https://openrouter.ai/api/v1',
  use_general_model: true,
  general_model: 'qwen/qwen3.5-flash-02-23',
  chat_model: 'qwen/qwen3.5-flash-02-23',
  vision_model: 'qwen/qwen3.5-flash-02-23',
  reasoning_model: 'qwen/qwen3.5-flash-02-23',
  embedding_model: 'qwen/qwen3-embedding-8b',
  stt_model: 'openai/whisper-large-v3',
  tts_model: 'openai/gpt-4o-mini-tts-2025-12-15',
  app_title: 'Study Buddy Pro',
  site_url: 'https://github.com/vindepemarte/study-buddy-pro',
} as const;

interface OpenRouterModel {
  id: string;
  name: string;
  architecture?: {
    input_modalities?: string[];
    output_modalities?: string[];
    modality?: string | null;
  };
  pricing?: {
    prompt?: string;
    completion?: string;
    image?: string;
    request?: string;
    web_search?: string;
    internal_reasoning?: string;
  };
  supported_parameters?: string[];
  top_provider?: {
    context_length?: number | null;
    max_completion_tokens?: number | null;
  } | null;
}

interface OpenRouterModelCatalog {
  configured: boolean;
  models: OpenRouterModel[];
}

/// Built-in prompt body is ~17 KB; cap roomy so users can edit without truncation.
const PROMPT_MAX_CHARS = 32000;
/// Default textarea height for the system prompt: large enough to show a
/// meaningful slice of the seeded built-in body without forcing the user to
/// drag the resize grip on first open.
const PROMPT_TEXTAREA_ROWS = 16;
const EJECT_RESET_MS = 2500;
/// Approximate tokens per chat turn used for the "~N turns of context" hint.
/// 400 tokens ≈ a typical user question + assistant reply pair on this app.
const TOKENS_PER_TURN_ESTIMATE = 400;

const KEEP_WARM_TOOLTIP =
  'Keep Warm holds your active model loaded in VRAM after each use. ' +
  'The timer below sets how long before it auto-releases; use -1 to keep it indefinitely. ' +
  'Unload now releases it immediately. ' +
  'If set to 0, Ollama unloads models after its default 5-minute timeout.';

// Log-scale context window slider: slider pos [0..1000] ↔ token count.
// Scale: value = CTX_MIN * (CTX_MAX / CTX_MIN)^(pos/1000)
// With CTX_MAX/CTX_MIN = 512 (= 2^9), each 1/9 of the slider doubles the value.
const CTX_MIN = 2048;
const CTX_MAX = 1_048_576; // 1M
const CTX_LOG_RATIO = Math.log(CTX_MAX / CTX_MIN);

function ctxToPos(v: number): number {
  return Math.round((1000 * Math.log(v / CTX_MIN)) / CTX_LOG_RATIO);
}

function posToCtx(pos: number): number {
  // Snap to nearest 1 KiB boundary (standard Ollama increment).
  return (
    Math.round((CTX_MIN * Math.pow(CTX_MAX / CTX_MIN, pos / 1000)) / 1024) *
    1024
  );
}

const CTX_TICKS = [
  '2K',
  '4K',
  '8K',
  '16K',
  '32K',
  '64K',
  '128K',
  '256K',
  '512K',
  '1M',
];

function hasModality(
  model: OpenRouterModel,
  direction: 'input' | 'output',
  value: string,
) {
  const modalities =
    direction === 'input'
      ? model.architecture?.input_modalities
      : model.architecture?.output_modalities;
  return modalities?.includes(value) ?? false;
}

function modelOptions(
  models: OpenRouterModel[],
  current: string,
  predicate: (model: OpenRouterModel) => boolean,
): string[] {
  const ids = models
    .filter(predicate)
    .map((model) => model.id)
    .sort((a, b) => a.localeCompare(b));
  if (current && !ids.includes(current)) ids.unshift(current);
  return ids.length > 0 ? ids : current ? [current] : [];
}

function modelLabel(model: OpenRouterModel | undefined): string {
  if (!model) return 'Not found in current catalog';
  const inputs = model.architecture?.input_modalities?.join('+') || 'unknown';
  const outputs = model.architecture?.output_modalities?.join('+') || 'unknown';
  const ctx = model.top_provider?.context_length
    ? ` · ${model.top_provider.context_length.toLocaleString()} ctx`
    : '';
  const params = model.supported_parameters?.slice(0, 4).join(', ');
  return `${inputs} -> ${outputs}${ctx}${params ? ` · ${params}` : ''}`;
}

function pricePerMillion(raw: string | undefined): string {
  const value = Number(raw ?? 0);
  return moneyPerMillion(value);
}

function moneyPerMillion(value: number): string {
  if (!Number.isFinite(value)) return '$0.00';
  return `$${(value * 1_000_000).toFixed(value === 0 ? 2 : 4)}`;
}

function modelById(models: OpenRouterModel[]): Map<string, OpenRouterModel> {
  return new Map(models.map((model) => [model.id, model]));
}

function modelPriceLine(model: OpenRouterModel | undefined): string {
  if (!model) return 'Pricing unavailable until the catalog loads.';
  const input = pricePerMillion(model.pricing?.prompt);
  const output = pricePerMillion(model.pricing?.completion);
  const image = pricePerMillion(model.pricing?.image);
  const request = pricePerMillion(model.pricing?.request);
  return `Input ${input}/1M · Output ${output}/1M · Image ${image}/1M · Request ${request}/1M`;
}

export function ModelTab({ config, resyncToken, onSaved }: ModelTabProps) {
  const openrouter = config.openrouter ?? DEFAULT_OPENROUTER;
  const provider = (config.inference.provider ?? 'ollama') as Provider;
  const voiceProvider = (config.voice.provider ??
    'supertonic') as VoiceProvider;
  const [inactivityMin, setInactivityMin] = useState(
    config.inference.keep_warm_inactivity_minutes,
  );
  const [rawMin, setRawMin] = useState(
    String(config.inference.keep_warm_inactivity_minutes),
  );
  const minFocusedRef = useRef(false);
  const [ejecting, setEjecting] = useState(false);
  const [loadedModel, setLoadedModel] = useState<string | null>(null);

  // Context window: committed value drives the debounced save; local slider
  // pos updates live on drag without committing on every pixel.
  const [numCtx, setNumCtx] = useState(config.inference.num_ctx);
  const [ctxPos, setCtxPos] = useState(() =>
    ctxToPos(config.inference.num_ctx),
  );
  const [ctxChip, setCtxChip] = useState(String(config.inference.num_ctx));
  const ctxDraggingRef = useRef(false);

  const [devOpen, setDevOpen] = useState(false);
  const [catalog, setCatalog] = useState<OpenRouterModelCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);

  const refreshOpenRouterModels = useCallback(async () => {
    if (!openrouter.api_key.trim()) {
      setCatalog(null);
      setCatalogError('Add an OpenRouter API key to load models.');
      return;
    }
    setCatalogLoading(true);
    setCatalogError(null);
    try {
      const next = await invoke<OpenRouterModelCatalog>(
        'openrouter_list_models',
      );
      setCatalog(next);
    } catch (err) {
      setCatalog(null);
      setCatalogError(err instanceof Error ? err.message : String(err));
    } finally {
      setCatalogLoading(false);
    }
  }, [openrouter.api_key]);

  useEffect(() => {
    if (!openrouter.api_key.trim()) return;
    void refreshOpenRouterModels();
  }, [openrouter.api_key, resyncToken, refreshOpenRouterModels]);

  const catalogModels = useMemo(() => catalog?.models ?? [], [catalog]);
  const catalogById = useMemo(() => modelById(catalogModels), [catalogModels]);
  const textModelOptions = useMemo(
    () =>
      modelOptions(catalogModels, openrouter.chat_model, (model) =>
        hasModality(model, 'output', 'text'),
      ),
    [catalogModels, openrouter.chat_model],
  );
  const generalModelOptions = useMemo(
    () =>
      modelOptions(catalogModels, openrouter.general_model, (model) =>
        hasModality(model, 'output', 'text'),
      ),
    [catalogModels, openrouter.general_model],
  );
  const visionModelOptions = useMemo(
    () =>
      modelOptions(
        catalogModels,
        openrouter.vision_model,
        (model) =>
          hasModality(model, 'input', 'image') &&
          hasModality(model, 'output', 'text'),
      ),
    [catalogModels, openrouter.vision_model],
  );
  const reasoningModelOptions = useMemo(
    () =>
      modelOptions(
        catalogModels,
        openrouter.reasoning_model,
        (model) =>
          hasModality(model, 'output', 'text') &&
          ((model.supported_parameters?.length ?? 0) === 0 ||
            Boolean(
              model.supported_parameters?.some((param) =>
                param.toLowerCase().includes('reasoning'),
              ),
            )),
      ),
    [catalogModels, openrouter.reasoning_model],
  );
  const embeddingModelOptions = useMemo(
    () =>
      modelOptions(catalogModels, openrouter.embedding_model, (model) =>
        hasModality(model, 'output', 'embeddings'),
      ),
    [catalogModels, openrouter.embedding_model],
  );
  const sttModelOptions = useMemo(
    () =>
      modelOptions(
        catalogModels,
        openrouter.stt_model,
        (model) =>
          hasModality(model, 'output', 'transcription') ||
          (hasModality(model, 'input', 'audio') &&
            hasModality(model, 'output', 'text')),
      ),
    [catalogModels, openrouter.stt_model],
  );
  const ttsModelOptions = useMemo(
    () =>
      modelOptions(
        catalogModels,
        openrouter.tts_model,
        (model) =>
          hasModality(model, 'output', 'speech') ||
          hasModality(model, 'output', 'audio'),
      ),
    [catalogModels, openrouter.tts_model],
  );

  const selectedGeneral = catalogById.get(openrouter.general_model);
  const selectedChat = catalogById.get(openrouter.chat_model);
  const selectedVision = catalogById.get(openrouter.vision_model);
  const selectedReasoning = catalogById.get(openrouter.reasoning_model);
  const selectedEmbedding = catalogById.get(openrouter.embedding_model);
  const selectedStt = catalogById.get(openrouter.stt_model);
  const selectedTts = catalogById.get(openrouter.tts_model);
  const selectedStack = useMemo(() => {
    const ids = openrouter.use_general_model
      ? [
          openrouter.general_model,
          openrouter.embedding_model,
          openrouter.stt_model,
          openrouter.tts_model,
        ]
      : [
          openrouter.chat_model,
          openrouter.vision_model,
          openrouter.reasoning_model,
          openrouter.embedding_model,
          openrouter.stt_model,
          openrouter.tts_model,
        ];
    return Array.from(new Set(ids.filter(Boolean)))
      .map((id) => catalogById.get(id))
      .filter((model): model is OpenRouterModel => Boolean(model));
  }, [
    catalogById,
    openrouter.use_general_model,
    openrouter.general_model,
    openrouter.chat_model,
    openrouter.vision_model,
    openrouter.reasoning_model,
    openrouter.embedding_model,
    openrouter.stt_model,
    openrouter.tts_model,
  ]);
  const selectedInputPerToken = selectedStack.reduce(
    (sum, model) => sum + Number(model.pricing?.prompt ?? 0),
    0,
  );
  const selectedOutputPerToken = selectedStack.reduce(
    (sum, model) => sum + Number(model.pricing?.completion ?? 0),
    0,
  );
  const catalogStatus = catalogLoading
    ? 'Loading OpenRouter models...'
    : catalogError
      ? catalogError
      : catalog
        ? `${catalog.models.length.toLocaleString()} models loaded from OpenRouter`
        : openrouter.api_key.trim()
          ? 'Model catalog not loaded yet.'
          : 'Add an OpenRouter API key to load model capabilities and pricing.';

  useEffect(() => {
    let unlistenLoaded: (() => void) | null = null;
    let unlistenEvicted: (() => void) | null = null;

    async function setup() {
      unlistenLoaded = await listen<string>('warmup:model-loaded', (e) => {
        setLoadedModel(e.payload);
      });
      unlistenEvicted = await listen<null>('warmup:model-evicted', () => {
        setLoadedModel(null);
      });
      invoke<string | null>('get_loaded_model')
        .then(setLoadedModel)
        .catch(() => {});
    }

    setup();

    function handleVisibilityChange() {
      if (!document.hidden) {
        invoke<string | null>('get_loaded_model')
          .then(setLoadedModel)
          .catch(() => {});
      }
    }
    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      unlistenLoaded?.();
      unlistenEvicted?.();
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, []);

  const { resetTo: resetMin } = useDebouncedSave(
    'inference',
    'keep_warm_inactivity_minutes',
    inactivityMin,
    { onSaved },
  );

  const { resetTo: resetNumCtx } = useDebouncedSave(
    'inference',
    'num_ctx',
    numCtx,
    { onSaved },
  );

  const prevTokenRef = useRef(resyncToken);

  if (prevTokenRef.current !== resyncToken) {
    prevTokenRef.current = resyncToken;
    if (!minFocusedRef.current) {
      setInactivityMin(config.inference.keep_warm_inactivity_minutes);
      setRawMin(String(config.inference.keep_warm_inactivity_minutes));
      resetMin(config.inference.keep_warm_inactivity_minutes);
    }
    const nextCtx = config.inference.num_ctx;
    setNumCtx(nextCtx);
    setCtxPos(ctxToPos(nextCtx));
    setCtxChip(String(nextCtx));
    resetNumCtx(nextCtx);
  }

  function commitCtx(v: number) {
    setNumCtx(v);
    setCtxPos(ctxToPos(v));
    setCtxChip(String(v));
  }

  function handleEject() {
    setEjecting(true);
    invoke('evict_model')
      .then(() => {
        setTimeout(() => setEjecting(false), EJECT_RESET_MS);
      })
      .catch(() => setEjecting(false));
  }

  const ctxTurns = Math.round(numCtx / TOKENS_PER_TURN_ESTIMATE);
  const fillPct = `${ctxPos / 10}%`;

  return (
    <>
      <Section heading="Provider">
        <SaveField
          section="inference"
          fieldKey="provider"
          label="Inference provider"
          helper={configHelp('inference', 'provider')}
          initialValue={provider}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <Dropdown<Provider>
              value={value}
              options={PROVIDER_OPTIONS}
              onChange={setValue}
              ariaLabel="Inference provider"
            />
          )}
        />
        <div className={styles.rowHelper}>
          {provider === 'openrouter'
            ? 'OpenRouter is active. Screenshot chat uses the selected vision/general model instead of the local Ollama picker.'
            : 'Ollama is active. OpenRouter settings stay saved and ready when you switch providers.'}
        </div>
      </Section>

      <Section heading="OpenRouter">
        <SaveField
          section="openrouter"
          fieldKey="api_key"
          label="API key"
          helper={configHelp('openrouter', 'api_key')}
          initialValue={openrouter.api_key}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue, errored) => (
            <TextField
              type="password"
              value={value}
              onChange={setValue}
              placeholder="sk-or-v1-..."
              errored={errored}
              ariaLabel="OpenRouter API key"
            />
          )}
        />
        <SaveField
          section="openrouter"
          fieldKey="base_url"
          label="Base URL"
          helper={configHelp('openrouter', 'base_url')}
          initialValue={openrouter.base_url}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue, errored) => (
            <TextField
              type="url"
              value={value}
              onChange={setValue}
              placeholder="https://openrouter.ai/api/v1"
              errored={errored}
              ariaLabel="OpenRouter base URL"
            />
          )}
        />

        <div className={styles.openRouterCatalogRow}>
          <div className={styles.rowHelper}>{catalogStatus}</div>
          <button
            type="button"
            className={styles.button}
            onClick={() => void refreshOpenRouterModels()}
            disabled={catalogLoading || !openrouter.api_key.trim()}
          >
            {catalogLoading ? 'Loading...' : 'Refresh models'}
          </button>
        </div>

        <SaveField
          section="openrouter"
          fieldKey="use_general_model"
          label="Use one model"
          helper={configHelp('openrouter', 'use_general_model')}
          initialValue={openrouter.use_general_model}
          resyncToken={resyncToken}
          onSaved={onSaved}
          rightAlign
          render={(value, setValue) => (
            <Toggle
              checked={value}
              onChange={setValue}
              ariaLabel="Use one OpenRouter model for chat, vision, and reasoning"
            />
          )}
        />

        <SaveField
          section="openrouter"
          fieldKey="general_model"
          label="General model"
          helper={configHelp('openrouter', 'general_model')}
          initialValue={openrouter.general_model}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <>
              <Dropdown<string>
                value={value}
                options={generalModelOptions}
                onChange={setValue}
                ariaLabel="OpenRouter general model"
              />
              <div className={styles.rowHelper}>
                {modelLabel(selectedGeneral)}
                {openrouter.use_general_model &&
                selectedGeneral &&
                !hasModality(selectedGeneral, 'input', 'image')
                  ? ' · no image input, so direct screenshot chat will fail'
                  : ''}
              </div>
              <div className={styles.rowHelper}>
                {modelPriceLine(selectedGeneral)}
              </div>
            </>
          )}
        />

        {!openrouter.use_general_model && (
          <>
            <SaveField
              section="openrouter"
              fieldKey="chat_model"
              label="Text chat"
              helper={configHelp('openrouter', 'chat_model')}
              initialValue={openrouter.chat_model}
              resyncToken={resyncToken}
              onSaved={onSaved}
              render={(value, setValue) => (
                <>
                  <Dropdown<string>
                    value={value}
                    options={textModelOptions}
                    onChange={setValue}
                    ariaLabel="OpenRouter text chat model"
                  />
                  <div className={styles.rowHelper}>
                    {modelLabel(selectedChat)}
                  </div>
                  <div className={styles.rowHelper}>
                    {modelPriceLine(selectedChat)}
                  </div>
                </>
              )}
            />
            <SaveField
              section="openrouter"
              fieldKey="vision_model"
              label="Vision"
              helper={configHelp('openrouter', 'vision_model')}
              initialValue={openrouter.vision_model}
              resyncToken={resyncToken}
              onSaved={onSaved}
              render={(value, setValue) => (
                <>
                  <Dropdown<string>
                    value={value}
                    options={visionModelOptions}
                    onChange={setValue}
                    ariaLabel="OpenRouter vision model"
                  />
                  <div className={styles.rowHelper}>
                    {modelLabel(selectedVision)}
                  </div>
                  <div className={styles.rowHelper}>
                    {modelPriceLine(selectedVision)}
                  </div>
                </>
              )}
            />
            <SaveField
              section="openrouter"
              fieldKey="reasoning_model"
              label="Reasoning"
              helper={configHelp('openrouter', 'reasoning_model')}
              initialValue={openrouter.reasoning_model}
              resyncToken={resyncToken}
              onSaved={onSaved}
              render={(value, setValue) => (
                <>
                  <Dropdown<string>
                    value={value}
                    options={reasoningModelOptions}
                    onChange={setValue}
                    ariaLabel="OpenRouter reasoning model"
                  />
                  <div className={styles.rowHelper}>
                    {modelLabel(selectedReasoning)}
                  </div>
                  <div className={styles.rowHelper}>
                    {modelPriceLine(selectedReasoning)}
                  </div>
                </>
              )}
            />
          </>
        )}

        <SaveField
          section="openrouter"
          fieldKey="embedding_model"
          label="Embeddings"
          helper={configHelp('openrouter', 'embedding_model')}
          initialValue={openrouter.embedding_model}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <>
              <Dropdown<string>
                value={value}
                options={embeddingModelOptions}
                onChange={setValue}
                ariaLabel="OpenRouter embedding model"
              />
              <div className={styles.rowHelper}>
                {modelLabel(selectedEmbedding)}
              </div>
              <div className={styles.rowHelper}>
                {modelPriceLine(selectedEmbedding)}
              </div>
            </>
          )}
        />
        <SaveField
          section="openrouter"
          fieldKey="stt_model"
          label="Speech to text"
          helper={configHelp('openrouter', 'stt_model')}
          initialValue={openrouter.stt_model}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <>
              <Dropdown<string>
                value={value}
                options={sttModelOptions}
                onChange={setValue}
                ariaLabel="OpenRouter speech-to-text model"
              />
              <div className={styles.rowHelper}>{modelLabel(selectedStt)}</div>
            </>
          )}
        />
        <SaveField
          section="openrouter"
          fieldKey="tts_model"
          label="Text to speech"
          helper={configHelp('openrouter', 'tts_model')}
          initialValue={openrouter.tts_model}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <>
              <Dropdown<string>
                value={value}
                options={ttsModelOptions}
                onChange={setValue}
                ariaLabel="OpenRouter text-to-speech model"
              />
              <div className={styles.rowHelper}>{modelLabel(selectedTts)}</div>
            </>
          )}
        />

        <div className={styles.priceSummaryGrid}>
          <div className={styles.priceSummaryCard}>
            <span className={styles.priceSummaryLabel}>Input</span>
            <strong>{moneyPerMillion(selectedInputPerToken)}/1M</strong>
          </div>
          <div className={styles.priceSummaryCard}>
            <span className={styles.priceSummaryLabel}>Output</span>
            <strong>{moneyPerMillion(selectedOutputPerToken)}/1M</strong>
          </div>
        </div>
      </Section>

      <Section heading="Ollama">
        <SaveField
          section="inference"
          fieldKey="ollama_url"
          label="Ollama URL"
          helper={configHelp('inference', 'ollama_url')}
          initialValue={config.inference.ollama_url}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue, errored) => (
            <TextField
              value={value}
              onChange={setValue}
              placeholder={config.inference.ollama_url}
              errored={errored}
              ariaLabel="Ollama URL"
            />
          )}
        />
      </Section>

      <Section heading="Keep Warm">
        {/* Row 1: label + [?] on left | Release after [N] min on right */}
        <div className={styles.keepWarmRow1}>
          <div className={styles.keepWarmLabelLine}>
            <span className={styles.keepWarmLabel}>
              Keep active model in VRAM
            </span>
            <Tooltip label={KEEP_WARM_TOOLTIP} multiline>
              <button
                type="button"
                className={styles.infoBtn}
                aria-label="About Keep active model in VRAM"
              >
                ?
              </button>
            </Tooltip>
          </div>
          <div className={styles.keepWarmTimerGroup}>
            <span className={styles.keepWarmBarFieldLabel}>Release after</span>
            <input
              type="number"
              className={styles.keepWarmNumberInput}
              value={rawMin}
              min={-1}
              max={1440}
              aria-label="Release after N minutes"
              onFocus={() => {
                minFocusedRef.current = true;
              }}
              onChange={(e) => {
                const n = parseInt(e.target.value, 10);
                if (Number.isNaN(n)) {
                  setRawMin(e.target.value);
                } else {
                  const clamped = Math.max(-1, Math.min(1440, n));
                  setRawMin(String(clamped));
                  setInactivityMin(clamped);
                }
              }}
              onBlur={() => {
                minFocusedRef.current = false;
                if (Number.isNaN(parseInt(rawMin, 10))) {
                  setRawMin('0');
                  setInactivityMin(0);
                }
              }}
            />
            <span className={styles.keepWarmUnit}>min</span>
          </div>
        </div>

        {/* Row 2: slug status on left | Unload now on right */}
        <div className={styles.keepWarmStatusRow}>
          <div className={styles.keepWarmStatusLeft}>
            {loadedModel !== null ? (
              <div className={styles.keepWarmVramSubtitle}>
                <span
                  className={styles.keepWarmVramDot}
                  data-testid="vram-status-dot"
                  aria-hidden="true"
                />
                <span className={styles.keepWarmVramModelName}>
                  {loadedModel}
                </span>
                <span>&nbsp;· in VRAM</span>
              </div>
            ) : (
              <span className={styles.keepWarmNoModel}>No model loaded</span>
            )}
          </div>

          <button
            type="button"
            className={styles.keepWarmEjectPill}
            aria-label="Unload now"
            disabled={ejecting || loadedModel === null}
            data-ejecting={ejecting}
            onClick={handleEject}
          >
            {ejecting ? (
              <DrawCheckIcon />
            ) : (
              <svg
                viewBox="0 0 16 16"
                width="11"
                height="11"
                fill="currentColor"
                aria-hidden="true"
              >
                <polygon points="8,2 14,11 2,11" />
                <rect x="2" y="12.5" width="12" height="2" rx="1" />
              </svg>
            )}
            Unload now
          </button>
        </div>
      </Section>

      <Section heading="Context Window">
        <div className={styles.ctxBlock}>
          {/* Label row: "Context window" left + editable token chip right */}
          <div className={styles.ctxTopRow}>
            <span className={styles.ctxLabel}>Context window</span>
            <div className={styles.ctxChipGroup}>
              <input
                type="number"
                className={styles.ctxChipInput}
                value={ctxChip}
                min={CTX_MIN}
                max={CTX_MAX}
                aria-label="Context window tokens"
                onChange={(e) => setCtxChip(e.target.value)}
                onBlur={() => {
                  const n = parseInt(ctxChip, 10);
                  if (!Number.isNaN(n) && n >= CTX_MIN) {
                    // Clamp upper bound so the UI mirrors the backend
                    // BOUNDS_NUM_CTX cap and the slider stays in sync.
                    commitCtx(Math.min(n, CTX_MAX));
                  } else {
                    setCtxChip(String(numCtx));
                  }
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
                }}
              />
              <span className={styles.ctxChipUnit}>tokens</span>
            </div>
          </div>

          {/* Log-scale slider — fill percentage tracked via CSS custom property */}
          <input
            type="range"
            className={styles.ctxSlider}
            style={{ '--fill': fillPct } as React.CSSProperties}
            min={0}
            max={1000}
            step={1}
            value={ctxPos}
            aria-label="Context window tokens"
            aria-valuemin={CTX_MIN}
            aria-valuemax={CTX_MAX}
            aria-valuenow={numCtx}
            aria-valuetext={`${numCtx} tokens`}
            onChange={(e) => {
              ctxDraggingRef.current = true;
              const pos = Number(e.target.value);
              setCtxPos(pos);
              setCtxChip(String(posToCtx(pos)));
            }}
            onMouseUp={() => {
              ctxDraggingRef.current = false;
              commitCtx(posToCtx(ctxPos));
            }}
            onTouchEnd={() => {
              ctxDraggingRef.current = false;
              commitCtx(posToCtx(ctxPos));
            }}
            onKeyUp={() => {
              if (!ctxDraggingRef.current) commitCtx(posToCtx(ctxPos));
            }}
          />

          <div className={styles.ctxTickRow} aria-hidden="true">
            {CTX_TICKS.map((label, i) => (
              <span
                key={label}
                className={styles.ctxTick}
                style={{ left: `${(i / (CTX_TICKS.length - 1)) * 100}%` }}
              >
                {label}
              </span>
            ))}
          </div>

          <div className={styles.ctxHelper}>
            ~{ctxTurns.toLocaleString()} turns of context
            {' · '}
            Ollama caps to your model&apos;s trained maximum.
          </div>

          <div className={styles.ctxVramNote}>
            <span className={styles.ctxVramIcon} aria-hidden="true">
              ⚠
            </span>
            <span>
              The KV cache scales linearly with context length, so doubling the
              context roughly doubles its memory footprint (model weights stay
              the same). Benchmark with your hardware before pushing it high.{' '}
              <button
                type="button"
                className={styles.ctxVramLink}
                onClick={() => {
                  void invoke('open_url', {
                    url: 'https://github.com/vindepemarte/study-buddy-pro/blob/main/docs/tuning-context-window.md#the-5-minute-benchmark-recipe',
                  });
                }}
              >
                Learn how to tune Context Window in 5 minute ↗
              </button>
            </span>
          </div>
        </div>
      </Section>

      <Section heading="Voice">
        <SaveField
          section="voice"
          fieldKey="enabled"
          label="Enable voice"
          helper="Let Study Buddy Pro speak tutor responses through the selected voice provider."
          initialValue={config.voice.enabled}
          resyncToken={resyncToken}
          onSaved={onSaved}
          rightAlign
          render={(value, setValue) => (
            <Toggle
              checked={value}
              onChange={setValue}
              ariaLabel="Enable voice"
            />
          )}
        />
        <SaveField
          section="voice"
          fieldKey="provider"
          label="Voice provider"
          helper={configHelp('voice', 'provider')}
          initialValue={voiceProvider}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <Dropdown
              value={value}
              options={VOICE_PROVIDER_OPTIONS}
              onChange={(next) => setValue(next as VoiceProvider)}
              ariaLabel="Voice provider"
            />
          )}
        />
        <SaveField
          section="voice"
          fieldKey="auto_speak_study"
          label="Auto-speak Study Mode"
          helper="Speak guided study steps, questions, and feedback automatically. Normal chat stays manual."
          initialValue={config.voice.auto_speak_study}
          resyncToken={resyncToken}
          onSaved={onSaved}
          rightAlign
          render={(value, setValue) => (
            <Toggle
              checked={value}
              onChange={setValue}
              ariaLabel="Auto-speak Study Mode"
            />
          )}
        />
        {voiceProvider === 'openrouter' && (
          <SaveField
            section="voice"
            fieldKey="openrouter_voice"
            label="OpenRouter voice"
            helper={configHelp('voice', 'openrouter_voice')}
            initialValue={config.voice.openrouter_voice ?? 'nova'}
            resyncToken={resyncToken}
            onSaved={onSaved}
            render={(value, setValue, errored) => (
              <TextField
                value={value}
                onChange={setValue}
                placeholder="nova"
                errored={errored}
                ariaLabel="OpenRouter voice"
              />
            )}
          />
        )}
        <SaveField
          section="voice"
          fieldKey="base_url"
          label="Supertonic URL"
          helper="Loopback URL for the local Supertonic TTS server."
          initialValue={config.voice.base_url}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue, errored) => (
            <TextField
              value={value}
              onChange={setValue}
              placeholder="http://127.0.0.1:7788"
              errored={errored}
              ariaLabel="Supertonic URL"
            />
          )}
        />
        <SaveField
          section="voice"
          fieldKey="voice"
          label="Voice"
          helper="Built-in or imported Supertonic voice name."
          initialValue={config.voice.voice}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue, errored) => (
            <TextField
              value={value}
              onChange={setValue}
              placeholder="M1"
              errored={errored}
              ariaLabel="Voice"
            />
          )}
        />
        <SaveField
          section="voice"
          fieldKey="lang"
          label="Language"
          helper='Use "auto" to let Study Buddy Pro infer the spoken language from each response.'
          initialValue={config.voice.lang}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue, errored) => (
            <TextField
              value={value}
              onChange={setValue}
              placeholder="auto"
              errored={errored}
              ariaLabel="Voice language"
            />
          )}
        />
        <SaveField
          section="voice"
          fieldKey="steps"
          label="Quality steps"
          helper="Higher values can sound better but take longer."
          initialValue={config.voice.steps}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <NumberStepper
              value={value}
              min={4}
              max={12}
              onChange={setValue}
              ariaLabel="Voice quality steps"
            />
          )}
        />
        <SaveField
          section="voice"
          fieldKey="speed"
          label="Speed"
          helper="Spoken speed multiplier."
          initialValue={config.voice.speed}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <NumberSlider
              value={value}
              min={0.7}
              max={2}
              step={0.05}
              onChange={setValue}
              ariaLabel="Voice speed"
            />
          )}
        />
        <SaveField
          section="voice"
          fieldKey="max_chunk_length"
          label="Chunk length"
          helper="Maximum text characters per TTS chunk for long study turns."
          initialValue={config.voice.max_chunk_length}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <NumberStepper
              value={value}
              min={80}
              max={1000}
              step={20}
              onChange={setValue}
              ariaLabel="Voice chunk length"
            />
          )}
        />
      </Section>

      <Section heading="Prompt">
        <SaveField
          section="prompt"
          fieldKey="system"
          label="System prompt"
          helper={configHelp('prompt', 'system')}
          vertical
          initialValue={config.prompt.system}
          resyncToken={resyncToken}
          onSaved={onSaved}
          render={(value, setValue) => (
            <>
              <Textarea
                value={value}
                onChange={setValue}
                placeholder="Persona prompt…"
                maxLength={PROMPT_MAX_CHARS}
                ariaLabel="System prompt"
                rows={PROMPT_TEXTAREA_ROWS}
              />
              <div className={styles.charCounter}>
                {value.length} / {PROMPT_MAX_CHARS}
              </div>
            </>
          )}
        />
      </Section>

      <div className={styles.devSection}>
        <button
          type="button"
          className={styles.devTrigger}
          aria-expanded={devOpen}
          aria-controls="dev-diagnostics"
          onClick={() => setDevOpen((o) => !o)}
        >
          <span className={styles.devTriggerLabel}>Diagnostics</span>
          <span className={styles.devTag}>DEV</span>
          <svg
            className={`${styles.devChevron} ${devOpen ? styles.devChevronOpen : ''}`}
            viewBox="0 0 10 10"
            fill="currentColor"
            aria-hidden
          >
            <path
              d="M3 2l4 3-4 3"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
              fill="none"
            />
          </svg>
        </button>
        {devOpen && (
          <div id="dev-diagnostics">
            <SaveField
              section="debug"
              fieldKey="trace_enabled"
              label="Trace recording"
              helper={configHelp('debug', 'trace_enabled')}
              initialValue={config.debug.trace_enabled}
              resyncToken={resyncToken}
              onSaved={onSaved}
              tooltipPlacement="top"
              rightAlign
              render={(value, setValue) => (
                <Toggle
                  checked={value}
                  onChange={setValue}
                  ariaLabel="Enable trace recording"
                />
              )}
            />
          </div>
        )}
      </div>
    </>
  );
}

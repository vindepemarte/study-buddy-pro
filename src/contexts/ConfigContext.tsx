/**
 * Application configuration context.
 *
 * Hydrates once from the Rust-side `get_config` Tauri command on mount, then
 * provides a synchronous `useConfig` hook to every descendant. Render is
 * gated until the first fetch resolves so components never see a null
 * config: this eliminates the per-call-site fallback literals that the
 * backend migration is specifically trying to kill.
 *
 * The Rust `AppConfig` serializes with snake_case field names (matching the
 * on-disk TOML schema). We translate to camelCase here so React components
 * keep their idiomatic JS names. The active model is NOT in this config:
 * Ollama's `/api/tags` is the source of truth and the active slug lives in
 * the Tauri-side `ActiveModelState`, surfaced through `useModelSelection`.
 */

import { createContext, use, useEffect, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * Backend event broadcast after the in-memory `AppConfig` is replaced
 * (`set_config_field`, `reset_config`, `reload_config_from_disk`). Mirrors
 * the Rust-side `CONFIG_UPDATED_EVENT` constant in `settings_commands.rs`.
 * Kept as a string literal here to avoid pulling a Rust-codegen dep into
 * the frontend.
 */
const CONFIG_UPDATED_EVENT = 'thuki://config-updated';

/** Shape returned by the Rust `get_config` command (snake_case). */
interface RawAppConfig {
  inference: {
    provider?: string;
    ollama_url: string;
  };
  openrouter?: {
    api_key: string;
    base_url: string;
    use_general_model: boolean;
    general_model: string;
    chat_model: string;
    vision_model: string;
    reasoning_model: string;
    embedding_model: string;
    stt_model: string;
    tts_model: string;
    app_title: string;
    site_url: string;
  };
  prompt: {
    system: string;
  };
  window: {
    overlay_width: number;
    max_chat_height: number;
    max_images: number;
    text_base_px: number;
    text_line_height: number;
    text_letter_spacing_px: number;
    text_font_weight: number;
  };
  quote: {
    max_display_lines: number;
    max_display_chars: number;
    max_context_length: number;
  };
  voice?: {
    enabled: boolean;
    auto_speak_study: boolean;
    base_url: string;
    voice: string;
    lang: string;
    steps: number;
    speed: number;
    max_chunk_length: number;
  };
}

/** Camel-cased, frontend-friendly view of the configuration. */
export interface AppConfig {
  inference: {
    provider: string;
    ollamaUrl: string;
  };
  openrouter: {
    configured: boolean;
    baseUrl: string;
    useGeneralModel: boolean;
    generalModel: string;
    chatModel: string;
    visionModel: string;
    reasoningModel: string;
    embeddingModel: string;
    sttModel: string;
    ttsModel: string;
  };
  prompt: {
    /** Raw user-editable persona prompt (may be empty). */
    system: string;
  };
  window: {
    overlayWidth: number;
    maxChatHeight: number;
    maxImages: number;
    textBasePx: number;
    textLineHeight: number;
    textLetterSpacingPx: number;
    textFontWeight: number;
  };
  quote: {
    maxDisplayLines: number;
    maxDisplayChars: number;
    maxContextLength: number;
  };
  voice: {
    enabled: boolean;
    autoSpeakStudy: boolean;
    baseUrl: string;
    voice: string;
    lang: string;
    steps: number;
    speed: number;
    maxChunkLength: number;
  };
}

function transform(raw: RawAppConfig): AppConfig {
  return {
    inference: {
      provider: raw.inference.provider ?? 'ollama',
      ollamaUrl: raw.inference.ollama_url,
    },
    openrouter: {
      configured: Boolean(raw.openrouter?.api_key?.trim()),
      baseUrl: raw.openrouter?.base_url ?? 'https://openrouter.ai/api/v1',
      useGeneralModel: raw.openrouter?.use_general_model ?? true,
      generalModel: raw.openrouter?.general_model ?? 'qwen/qwen3.5-flash-02-23',
      chatModel: raw.openrouter?.chat_model ?? 'qwen/qwen3.5-flash-02-23',
      visionModel: raw.openrouter?.vision_model ?? 'qwen/qwen3.5-flash-02-23',
      reasoningModel:
        raw.openrouter?.reasoning_model ?? 'qwen/qwen3.5-flash-02-23',
      embeddingModel:
        raw.openrouter?.embedding_model ?? 'qwen/qwen3-embedding-8b',
      sttModel: raw.openrouter?.stt_model ?? 'openai/whisper-large-v3',
      ttsModel:
        raw.openrouter?.tts_model ?? 'openai/gpt-4o-mini-tts-2025-12-15',
    },
    prompt: {
      system: raw.prompt.system,
    },
    window: {
      overlayWidth: raw.window.overlay_width,
      maxChatHeight: raw.window.max_chat_height,
      maxImages: raw.window.max_images,
      textBasePx: raw.window.text_base_px,
      textLineHeight: raw.window.text_line_height,
      textLetterSpacingPx: raw.window.text_letter_spacing_px,
      textFontWeight: raw.window.text_font_weight,
    },
    quote: {
      maxDisplayLines: raw.quote.max_display_lines,
      maxDisplayChars: raw.quote.max_display_chars,
      maxContextLength: raw.quote.max_context_length,
    },
    voice: {
      enabled: raw.voice?.enabled ?? false,
      autoSpeakStudy: raw.voice?.auto_speak_study ?? false,
      baseUrl: raw.voice?.base_url ?? 'http://127.0.0.1:7788',
      voice: raw.voice?.voice ?? 'M1',
      lang: raw.voice?.lang ?? 'auto',
      steps: raw.voice?.steps ?? 8,
      speed: raw.voice?.speed ?? 1.05,
      maxChunkLength: raw.voice?.max_chunk_length ?? 300,
    },
  };
}

const ConfigContext = createContext<AppConfig | null>(null);

/**
 * Renders children only once `get_config` resolves. Blocks with `null`
 * (no visible splash) for the tiny IPC round-trip; Tauri local IPC is
 * sub-10ms in practice.
 */
export function ConfigProvider({ children }: { children: ReactNode }) {
  const [config, setConfig] = useState<AppConfig | null>(null);

  useEffect(() => {
    let cancelled = false;

    /**
     * Fetches `get_config` and pushes the transformed value into state. Used
     * for initial hydrate and for every `CONFIG_UPDATED_EVENT` refresh after
     * the Settings window writes a change. The post-mount path tolerates
     * the same nullish/error fallbacks as initial mount: a transient IPC
     * failure should not flip the tree back to DEFAULT_CONFIG and lose any
     * good values already in place.
     */
    const refresh = (initial: boolean) => {
      void invoke<RawAppConfig>('get_config')
        .then((raw) => {
          if (cancelled) return;
          if (raw == null) {
            if (initial) setConfig(DEFAULT_CONFIG);
            return;
          }
          setConfig(transform(raw));
        })
        .catch(() => {
          if (cancelled) return;
          if (initial) setConfig(DEFAULT_CONFIG);
        });
    };

    refresh(true);

    let unlisten: UnlistenFn | undefined;
    void listen<unknown>(CONFIG_UPDATED_EVENT, () => {
      refresh(false);
    })
      .then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch(() => {
        // Event bridge unavailable (test env, Tauri not ready). Initial
        // hydrate still happened above; subscribers fall back to a static
        // snapshot and pick up edits on next mount.
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (!config) return null;

  return <ConfigContext value={config}>{children}</ConfigContext>;
}

/**
 * Returns the current resolved `AppConfig`.
 *
 * When no `ConfigProvider` wraps the calling component, falls back to
 * `DEFAULT_CONFIG`. In production `main.tsx` always wraps `<App />`, so this
 * path only fires from component tests that render a leaf without setting up
 * a provider. Keeps test infrastructure minimal without compromising the
 * production single-source-of-truth guarantee.
 *
 * If test-side defaults ever drift from the Rust-side `AppConfig::default()`,
 * the fix is to update `DEFAULT_CONFIG` below. The two shapes are kept in
 * sync by hand because cross-language codegen is not worth the dependency
 * in a macOS-only desktop app.
 */
export function useConfig(): AppConfig {
  const value = use(ConfigContext);
  return value ?? DEFAULT_CONFIG;
}

/**
 * Test helper: wraps children with a synchronous (no `invoke`) ConfigContext
 * populated from `value`. Useful when a test needs to assert behavior against
 * a non-default config.
 */
export function ConfigProviderForTest({
  value,
  children,
}: {
  value: AppConfig;
  children: ReactNode;
}) {
  return <ConfigContext value={value}>{children}</ConfigContext>;
}

/**
 * Default AppConfig used when no `ConfigProvider` wraps the caller. Values
 * mirror the Rust-side `AppConfig::default()` (see
 * `src-tauri/src/config/defaults.rs`).
 */
export const DEFAULT_CONFIG: AppConfig = {
  inference: {
    provider: 'ollama',
    ollamaUrl: 'http://127.0.0.1:11434',
  },
  openrouter: {
    configured: false,
    baseUrl: 'https://openrouter.ai/api/v1',
    useGeneralModel: true,
    generalModel: 'qwen/qwen3.5-flash-02-23',
    chatModel: 'qwen/qwen3.5-flash-02-23',
    visionModel: 'qwen/qwen3.5-flash-02-23',
    reasoningModel: 'qwen/qwen3.5-flash-02-23',
    embeddingModel: 'qwen/qwen3-embedding-8b',
    sttModel: 'openai/whisper-large-v3',
    ttsModel: 'openai/gpt-4o-mini-tts-2025-12-15',
  },
  prompt: { system: '' },
  window: {
    overlayWidth: 600,
    maxChatHeight: 648,
    maxImages: 3,
    textBasePx: 15,
    textLineHeight: 1.5,
    textLetterSpacingPx: 0,
    textFontWeight: 500,
  },
  quote: {
    maxDisplayLines: 4,
    maxDisplayChars: 300,
    maxContextLength: 4096,
  },
  voice: {
    enabled: true,
    autoSpeakStudy: true,
    baseUrl: 'http://127.0.0.1:7788',
    voice: 'M1',
    lang: 'auto',
    steps: 8,
    speed: 1.05,
    maxChunkLength: 300,
  },
};

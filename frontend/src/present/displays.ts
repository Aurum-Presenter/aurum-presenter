/**
 * Getting the audience window onto the projector.
 *
 * Three mechanisms, strongest first, and the weakest one always works: the app degrades until
 * something is on screen rather than failing with a permission error at the moment a service
 * starts (presenter-output journey).
 */

export type OutputRoute = 'window-management' | 'presentation-api' | 'manual';

export interface OpenedOutput {
  route: OutputRoute;
  window: Window | null;
  /** Set when the Presentation API is carrying the output instead of a window we opened. */
  connection: { terminate: () => void } | null;
  hint: string | null;
}

interface ScreenDetail {
  isPrimary: boolean;
  availLeft: number;
  availTop: number;
  availWidth: number;
  availHeight: number;
  label: string;
}

interface ScreenDetails {
  screens: ScreenDetail[];
}

export async function openAudience(url: string): Promise<OpenedOutput> {
  const external = await externalScreen();

  if (external !== null) {
    const features = [
      `left=${external.availLeft}`,
      `top=${external.availTop}`,
      `width=${external.availWidth}`,
      `height=${external.availHeight}`,
      'noopener',
    ].join(',');

    const opened = window.open(url, `aurum-audience`, features);

    if (opened !== null) {
      return {
        route: 'window-management',
        window: opened,
        connection: null,
        hint: `Opened on ${external.label}. Press F in that window if it is not full screen.`,
      };
    }
  }

  const cast = await castTo(url);

  if (cast !== null) {
    return { route: 'presentation-api', window: null, connection: cast, hint: null };
  }

  // Nothing clever is available. A plain window, and an honest instruction.
  const opened = window.open(url, 'aurum-audience', 'width=1280,height=720');

  return {
    route: 'manual',
    window: opened,
    connection: null,
    hint: 'Drag this window to the projector and press F to go full screen.',
  };
}

/** The screen that is not the one the operator is sitting in front of. */
async function externalScreen(): Promise<ScreenDetail | null> {
  const api = (window as unknown as { getScreenDetails?: () => Promise<ScreenDetails> }).getScreenDetails;

  if (api === undefined) {
    return null;
  }

  try {
    const details = await api.call(window);

    return details.screens.find((screen) => ! screen.isPrimary) ?? null;
  } catch {
    // Permission refused, or no second screen. Both fall through to the next mechanism.
    return null;
  }
}

async function castTo(url: string): Promise<{ terminate: () => void } | null> {
  const Request = (window as unknown as {
    PresentationRequest?: new (urls: string[]) => { start: () => Promise<{ terminate: () => void }> };
  }).PresentationRequest;

  if (Request === undefined) {
    return null;
  }

  try {
    return await new Request([url]).start();
  } catch {
    // The user cancelled the picker, or there is no receiver on the network.
    return null;
  }
}

/** Full screen from inside the output window, where the gesture requirement is satisfiable. */
export function goFullscreen(): void {
  void document.documentElement.requestFullscreen?.().catch(() => undefined);
}

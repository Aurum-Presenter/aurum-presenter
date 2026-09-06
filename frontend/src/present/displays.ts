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

/**
 * The screen that is not the one the operator is sitting in front of.
 *
 * `isExtended` is readable without any permission, so a laptop with one screen is never asked
 * for one: the prompt only appears when there is actually a second screen to put the audience
 * window on. The call is raced with a timer because a prompt nobody answers must not hold up
 * the start of a service — the plain window is one keystroke from full screen anyway.
 */
async function externalScreen(): Promise<ScreenDetail | null> {
  const api = (window as unknown as { getScreenDetails?: () => Promise<ScreenDetails> }).getScreenDetails;

  if (api === undefined || (screen as Screen & { isExtended?: boolean }).isExtended !== true) {
    return null;
  }

  try {
    const details = await Promise.race([
      api.call(window),
      new Promise<null>((resolve) => setTimeout(() => resolve(null), 10_000)),
    ]);

    return details?.screens.find((each) => ! each.isPrimary) ?? null;
  } catch {
    // Permission refused, or no second screen. Both fall through to the next mechanism.
    return null;
  }
}

/**
 * Casting, but only when there is something to cast to.
 *
 * `start()` opens the browser's device picker and waits for a person, so it must never be
 * called speculatively: an operator with no Chromecast on the network would get a dialog in
 * front of them at the moment a service starts, and the audience window would wait behind it.
 * Availability is asked first, and briefly — discovery that has not answered in a second and a
 * half is treated as "no receiver", because a plain window now beats a cast screen later.
 */
async function castTo(url: string): Promise<{ terminate: () => void } | null> {
  const Request = (window as unknown as {
    PresentationRequest?: new (urls: string[]) => {
      start: () => Promise<{ terminate: () => void }>;
      getAvailability?: () => Promise<{ value: boolean }>;
    };
  }).PresentationRequest;

  if (Request === undefined) {
    return null;
  }

  try {
    const request = new Request([url]);

    if (request.getAvailability === undefined) {
      return null;
    }

    const available = await Promise.race([
      request.getAvailability().then((availability) => availability.value).catch(() => false),
      new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 1500)),
    ]);

    return available ? await request.start() : null;
  } catch {
    // The user cancelled the picker, or discovery is not supported here.
    return null;
  }
}

/** Full screen from inside the output window, where the gesture requirement is satisfiable. */
export function goFullscreen(): void {
  void document.documentElement.requestFullscreen?.().catch(() => undefined);
}

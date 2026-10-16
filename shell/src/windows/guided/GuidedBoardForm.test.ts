import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { fakeShell } from '../../test/fakeShell';
import type { FakeRoute } from '../../test/fakeFetch';
import { MOTOR_ON_UNO, SONAR_ON_UNO, TEMPLATES, UNO_PINS } from '../../test/guidedFixtures';
import GuidedBoardForm from './GuidedBoardForm.svelte';
import type { ArduinoInfo, GuidedBoard, UserRole } from '../../lib/state/types';

const INFO: ArduinoInfo = {
  tools: { state: 'ready', installing: false, cli: { path: '/x/arduino-cli', source: 'fake' } },
  boards: [
    { id: 'uno', name: 'Arduino Uno', chip: 'ATmega328P', fqbn: 'arduino:avr:uno', room: 32256 },
    {
      id: 'mega2560',
      name: 'Arduino Mega 2560',
      chip: 'ATmega2560',
      fqbn: 'arduino:avr:mega',
      room: 253952,
    },
  ],
  templates: [],
  sketchbook: 'home:/Arduino',
};

const EMPTY: GuidedBoard = {
  name: 'uno',
  folder: 'home:/Arduino/uno',
  sketch: 'home:/Arduino/uno/uno.ino',
  plan: { board: 'uno', name: 'uno', parts: {} },
  problems: [],
  wiring: [],
  generated: true,
  edited: false,
};

const FRONT = { template: 'ultrasonic', pins: { trig: 'D4', echo: 'D7' }, settings: {} };

const SAVED: GuidedBoard = {
  ...EMPTY,
  plan: { board: 'uno', name: 'uno', parts: { front: { ...FRONT, shape: 'trig-echo' } } },
  wiring: [
    {
      part: 'front',
      template: 'ultrasonic',
      lines: ['VCC → 5V', 'GND → GND', 'Trig → D4', 'Echo → D7'],
      warnings: ['Point two sonars apart.'],
    },
  ],
};

const PLAN = '/api/v0/arduino/plans/uno';

function setup(
  routes: FakeRoute[] = [],
  opts: { initialPlan?: string; role?: UserRole; plan?: GuidedBoard } = {},
) {
  const shell = fakeShell([
    { path: '/api/v0/arduino', body: INFO },
    {
      path: '/api/v0/arduino/plans',
      body: [{ name: 'uno', board: 'uno', parts: 0, takenOver: false }],
    },
    { path: '/api/v0/arduino/parts', body: TEMPLATES },
    { path: '/api/v0/arduino/pins/uno', body: UNO_PINS },
    { path: PLAN, body: opts.plan ?? EMPTY },
    { method: 'POST', path: `${PLAN}/suggest`, body: SONAR_ON_UNO },
    { method: 'PUT', path: PLAN, body: SAVED },
    ...routes,
  ]);
  if (opts.role) shell.signInAs(opts.role);
  const onshow = vi.fn();
  const oncancel = vi.fn();
  const r = render(GuidedBoardForm, {
    props: { initialPlan: opts.initialPlan ?? null, oncancel, onshow, onport: () => {} },
    context: shell.context,
  });
  return { shell, onshow, oncancel, ...r };
}

const calls = (shell: ReturnType<typeof fakeShell>, method: string, path: string) =>
  shell.fetch.calls.filter((c) => c.method === method && c.path === path);

/** From the board step to the part step, on the existing Uno. */
async function toPart(user: ReturnType<typeof userEvent.setup>) {
  await screen.findByRole('radio', { name: /^uno/ });
  await user.click(screen.getByRole('button', { name: 'Next' }));
  await screen.findByText('What goes on uno?');
}

afterEach(() => localStorage.clear());

describe('Add part → On a board’s pins', () => {
  it('adds an ultrasonic sensor on the suggested pins, saves the plan and shows the wiring', async () => {
    const user = userEvent.setup();
    const { shell } = setup();
    await toPart(user);
    // The catalogue, grouped by what parts do.
    expect(screen.getByText('Distance and obstacles')).toBeInTheDocument();
    expect(screen.getByText('Motors and movement')).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: /^Ultrasonic distance sensor/ }));
    // A sensible default name, and the template's settings with their units.
    expect(screen.getByLabelText('Name')).toHaveValue('front');
    expect(screen.getByLabelText('Longest distance')).toHaveValue('4');
    await user.click(screen.getByRole('button', { name: 'Choose pins' }));

    await screen.findByText('Trig is on D4. Pick another pin to move it.');
    const [ask] = calls(shell, 'POST', `${PLAN}/suggest`);
    expect(ask!.body).toEqual({
      template: 'ultrasonic',
      shape: 'trig-echo',
      name: 'front',
      pins: {},
      settings: { 'min-m': 0.02, 'max-m': 4 },
    });
    await user.click(screen.getByRole('button', { name: 'Save the plan' }));

    await screen.findByText('Wiring uno');
    const [put] = calls(shell, 'PUT', PLAN);
    expect(put!.body).toEqual({
      plan: {
        board: 'uno',
        name: 'uno',
        parts: {
          front: {
            template: 'ultrasonic',
            shape: 'trig-echo',
            pins: { trig: 'D4', echo: 'D7' },
            settings: { 'min-m': 0.02, 'max-m': 4 },
          },
        },
      },
    });
    expect(screen.getByRole('checkbox', { name: 'Trig → D4' })).not.toBeChecked();
    // Only new wires: nothing "changes".
    expect(screen.queryByText('Wires that change')).toBeNull();

    await user.click(screen.getByRole('button', { name: 'Next: upload' }));
    expect(await screen.findByRole('button', { name: /Verify/ })).toBeEnabled();
  });

  it('asks for the driver shape of a motor, with the drivers it fits', async () => {
    const user = userEvent.setup();
    const { shell } = setup([{ method: 'POST', path: `${PLAN}/suggest`, body: MOTOR_ON_UNO }]);
    await toPart(user);
    await user.click(screen.getByRole('radio', { name: /^DC motor with a driver/ }));
    const shapes = screen.getByRole('group', { name: 'How is it wired?' });
    expect(within(shapes).getAllByRole('radio')).toHaveLength(3);
    await user.click(within(shapes).getByRole('radio', { name: /L298N, L293D, TB6612/ }));
    expect(within(shapes).getByText('EN · IN1 · IN2 · STBY (optional)')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Choose pins' }));
    await screen.findByText('EN is on D5. Pick another pin to move it.');
    expect(calls(shell, 'POST', `${PLAN}/suggest`)[0]!.body).toMatchObject({
      template: 'motor',
      shape: 'en-in1-in2',
      name: 'left',
    });
    // The own-supply warning shows before saving.
    expect(screen.getByText(/needs its own supply/)).toBeInTheDocument();
  });

  it('checks the name and the settings while typing', async () => {
    const user = userEvent.setup();
    setup();
    await toPart(user);
    await user.click(screen.getByRole('radio', { name: /^Ultrasonic distance sensor/ }));
    const next = screen.getByRole('button', { name: 'Choose pins' });
    const name = screen.getByLabelText('Name');
    await user.clear(name);
    await user.type(name, 'Front');
    expect(name).toHaveAttribute('aria-invalid', 'true');
    expect(next).toBeDisabled();
    await user.clear(name);
    await user.type(name, 'front');
    expect(next).toBeEnabled();
    const longest = screen.getByLabelText('Longest distance');
    await user.clear(longest);
    await user.type(longest, '9');
    expect(screen.getByRole('alert')).toHaveTextContent('Longest distance: from 0.1 to 4 m.');
    expect(next).toBeDisabled();
  });

  it('says why the core refused the plan, problem by problem, with part, role and pin', async () => {
    const user = userEvent.setup();
    setup([
      {
        method: 'PUT',
        path: PLAN,
        status: 422,
        body: {
          error: {
            code: 'invalid-plan',
            message: 'The plan has 2 problems.',
            hint: 'Fix them and save again.',
            problems: [
              {
                severity: 'error',
                code: 'pin-used-twice',
                part: 'front',
                role: 'trig',
                pin: 'D4',
                message: 'D4 is used twice.',
                hint: 'Pick another pin.',
              },
              {
                severity: 'error',
                code: 'servo-timer-conflict',
                part: 'front',
                role: 'echo',
                pin: 'D7',
                message: 'A servo stops PWM on D7.',
                hint: 'Move it.',
              },
            ],
          },
        },
      },
    ]);
    await toPart(user);
    await user.click(screen.getByRole('radio', { name: /^Ultrasonic distance sensor/ }));
    await user.click(screen.getByRole('button', { name: 'Choose pins' }));
    await user.click(await screen.findByRole('button', { name: 'Save the plan' }));
    const list = await screen.findByRole('list', { name: 'Why the plan wasn’t saved' });
    const items = within(list).getAllByRole('listitem');
    expect(items[0]).toHaveTextContent('front · Trig · D4 D4 is used twice. Pick another pin.');
    expect(items[1]).toHaveTextContent('front · Echo · D7');
    // Still on the pins, to fix them.
    expect(screen.getByRole('button', { name: 'Save the plan' })).toBeEnabled();
  });

  it('asks before replacing a sketch changed outside Faber', async () => {
    const user = userEvent.setup();
    const { shell } = setup([
      {
        method: 'PUT',
        path: PLAN,
        status: 409,
        body: {
          error: {
            code: 'sketch-edited',
            message: 'uno.ino was changed outside Faber.',
            hint: 'Take over the code, or replace it.',
          },
        },
      },
    ]);
    await toPart(user);
    await user.click(screen.getByRole('radio', { name: /^Ultrasonic distance sensor/ }));
    await user.click(screen.getByRole('button', { name: 'Choose pins' }));
    await user.click(await screen.findByRole('button', { name: 'Save the plan' }));
    await screen.findByText('uno.ino was changed outside Faber.');
    shell.fetch.route({ method: 'PUT', path: PLAN, body: SAVED });
    await user.click(screen.getByRole('button', { name: 'Replace the changed sketch' }));
    await screen.findByText('Wiring uno');
    expect(calls(shell, 'PUT', PLAN).at(-1)!.body).toMatchObject({ overwriteEdits: true });
  });

  it('starts a new board: its type and a name checked against the sketches there are', async () => {
    const user = userEvent.setup();
    const mega: GuidedBoard = {
      ...EMPTY,
      name: 'mega',
      sketch: 'home:/Arduino/mega/mega.ino',
      plan: { board: 'mega2560', name: 'mega', parts: {} },
    };
    const { shell } = setup([
      { method: 'POST', path: '/api/v0/arduino/plans', status: 201, body: mega },
      { path: '/api/v0/arduino/pins/mega2560', body: UNO_PINS },
    ]);
    await user.click(await screen.findByRole('radio', { name: /A new board/ }));
    await user.selectOptions(screen.getByLabelText('Board type'), 'mega2560');
    const name = screen.getByLabelText('Board name');
    expect(name).toHaveValue('mega');
    await user.clear(name);
    await user.type(name, 'uno');
    expect(screen.getByText(/There is already a sketch called uno/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Next' })).toBeDisabled();
    await user.clear(name);
    await user.type(name, 'mega');
    await user.click(screen.getByRole('button', { name: 'Next' }));
    await screen.findByText('What goes on mega?');
    expect(calls(shell, 'POST', '/api/v0/arduino/plans')[0]!.body).toEqual({
      name: 'mega',
      board: 'mega2560',
    });
  });

  it('lets a viewer look but not save, and an operator save but not upload', async () => {
    const user = userEvent.setup();
    setup([], { role: 'viewer' });
    await toPart(user);
    await user.click(screen.getByRole('radio', { name: /^Ultrasonic distance sensor/ }));
    await user.click(screen.getByRole('button', { name: 'Choose pins' }));
    const save = await screen.findByRole('button', { name: 'Save the plan' });
    expect(save).toBeDisabled();
    expect(save).toHaveAttribute('title', 'Needs the operator role');
  });

  it('lets an operator verify the sketch, but uploading needs an admin', async () => {
    const user = userEvent.setup();
    setup([], { role: 'operator', initialPlan: 'uno', plan: SAVED });
    await user.click(await screen.findByRole('button', { name: 'Upload' }));
    const upload = await screen.findByRole('button', { name: /^Upload/ });
    await waitFor(() => expect(screen.getByRole('button', { name: /Verify/ })).toBeEnabled());
    expect(upload).toBeDisabled();
    expect(upload).toHaveAttribute('title', 'Needs the admin role');
  });
});

describe('editing a guided board’s plan', () => {
  const WITH_FRONT: GuidedBoard = {
    ...SAVED,
    plan: { board: 'uno', name: 'uno', parts: { front: FRONT } },
  };

  it('shows the parts with their pins, and removing one shows the wires that go', async () => {
    const user = userEvent.setup();
    const { shell } = setup([{ method: 'PUT', path: PLAN, body: EMPTY }], {
      initialPlan: 'uno',
      plan: WITH_FRONT,
    });
    expect(await screen.findByText('Trig D4 · Echo D7')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Remove front from the plan' }));
    await user.click(screen.getByRole('button', { name: 'Remove' }));
    await screen.findByText('Wires that change');
    expect(screen.getByText('front Trig: off D4')).toBeInTheDocument();
    expect(calls(shell, 'PUT', PLAN)[0]!.body).toEqual({
      plan: { board: 'uno', name: 'uno', parts: {} },
    });
  });

  it('moves a part’s pins: the suggestion edits that part, keeping its pins', async () => {
    const user = userEvent.setup();
    const { shell } = setup([], { initialPlan: 'uno', plan: WITH_FRONT });
    await user.click(await screen.findByRole('button', { name: 'Move the pins of front' }));
    await screen.findByText('Trig is on D4. Pick another pin to move it.');
    expect(calls(shell, 'POST', `${PLAN}/suggest`)[0]!.body).toMatchObject({
      name: 'front',
      pins: { trig: 'D4', echo: 'D7' },
    });
    await user.click(screen.getByRole('button', { name: 'D8, suitable for Trig' }));
    await waitFor(() =>
      expect(calls(shell, 'POST', `${PLAN}/suggest`).at(-1)!.body).toMatchObject({
        pins: { trig: 'D8', echo: 'D7' },
      }),
    );
  });
});

describe('the wiring checklist', () => {
  it('keeps the ticks per board in this browser', async () => {
    const user = userEvent.setup();
    const first = setup([], { initialPlan: 'uno', plan: SAVED });
    await user.click(await screen.findByRole('button', { name: 'Wiring' }));
    await user.click(screen.getByRole('checkbox', { name: 'Trig → D4' }));
    expect(screen.getByText('1 of 4 wires done')).toBeInTheDocument();
    first.unmount();
    const second = setup([], { initialPlan: 'uno', plan: SAVED });
    await user.click(await screen.findByRole('button', { name: 'Wiring' }));
    expect(screen.getByRole('checkbox', { name: 'Trig → D4' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Echo → D7' })).not.toBeChecked();
    // "Open in a window" opens the checklist on its own, to keep beside the board.
    await user.click(screen.getByRole('button', { name: /Open in a window/ }));
    const win = second.shell.desktop.state.windows.find((w) => w.kind === 'wiring');
    expect(win?.params).toEqual({ plan: 'uno' });
  });
});

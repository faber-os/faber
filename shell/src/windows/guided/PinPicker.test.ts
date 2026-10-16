import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import PinPicker from './PinPicker.svelte';
import { MOTOR_ON_UNO, SONAR_ON_UNO, UNO_PINS } from '../../test/guidedFixtures';
import type { RoleOptions } from '../../lib/state/types';

function setup(
  roles: RoleOptions[] = SONAR_ON_UNO.roles,
  pins: Record<string, string> = { trig: 'D4', echo: 'D7' },
) {
  const onpick = vi.fn();
  const onclear = vi.fn();
  render(PinPicker, {
    props: {
      map: UNO_PINS,
      roles,
      pins,
      firstPick: pins,
      current: roles[0]!.role,
      onpick,
      onclear,
    },
  });
  return { onpick, onclear };
}

const pin = (name: string) =>
  document.querySelector<SVGGElement>(`[data-pin="${name}"]`) ??
  (() => {
    throw new Error(`no pin ${name}`);
  })();

describe('the pin picker', () => {
  it('draws every pin of the board as a button that says what it is for the role', () => {
    setup();
    expect(document.querySelectorAll('[data-pin]')).toHaveLength(20);
    expect(screen.getByRole('button', { name: 'D4, chosen for Trig' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByRole('button', { name: 'D8, suitable for Trig' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', {
        name: 'D0, reserved: D0 and D1 carry the USB link to Faber (Serial).',
      }),
    ).toHaveAttribute('aria-disabled', 'true');
    // Labels like GND are drawn, but aren't pins.
    expect(screen.queryByRole('button', { name: /^GND/ })).toBeNull();
    expect(screen.getByText('Trig is on D4. Pick another pin to move it.')).toBeInTheDocument();
  });

  it('shows pins in use by other parts and pins that can’t do the job, with why', () => {
    const role: RoleOptions = {
      ...SONAR_ON_UNO.roles[0]!,
      inUse: [{ pin: 'D9', by: 'rear (Trig)' }],
      unsuitable: [{ pin: 'A0', reason: 'Trig needs a digital pin.' }],
    };
    setup([role, SONAR_ON_UNO.roles[1]!]);
    expect(pin('D9').dataset.kind).toBe('in-use');
    expect(pin('D9')).toHaveAccessibleName('D9, in use: rear (Trig)');
    expect(pin('A0').dataset.kind).toBe('unsuitable');
    expect(pin('D7').dataset.kind).toBe('other-role');
    // The legend lists what is on the drawing.
    expect(screen.getByRole('list', { name: 'What the colours mean' })).toHaveTextContent(
      /In use.*Reserved.*Can’t do this/,
    );
  });

  it('puts a clicked pin on the role, and only a pin that can do it', async () => {
    const user = userEvent.setup();
    const { onpick } = setup();
    await user.click(pin('D8'));
    expect(onpick).toHaveBeenCalledWith('trig', 'D8');
    await user.click(pin('D0'));
    expect(onpick).toHaveBeenCalledTimes(1);
    // The reason is written out next to the drawing.
    expect(screen.getByText(/^D0, reserved: /, { selector: '.info' })).toBeInTheDocument();
    // Echo's pin swaps.
    await user.click(pin('D7'));
    expect(onpick).toHaveBeenLastCalledWith('trig', 'D7');
  });

  it('moves along the headers with the arrow keys and picks with Enter', async () => {
    const user = userEvent.setup();
    const { onpick } = setup();
    // One tab stop: the role's pin.
    expect(pin('D4')).toHaveAttribute('tabindex', '0');
    expect(pin('D5')).toHaveAttribute('tabindex', '-1');
    pin('D4').focus();
    await user.keyboard('{ArrowLeft}');
    expect(pin('D5')).toHaveFocus();
    await user.keyboard('{ArrowRight}{ArrowRight}');
    expect(pin('D3')).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(onpick).toHaveBeenCalledWith('trig', 'D3');
    await user.keyboard('{End}');
    expect(pin('A5')).toHaveFocus();
  });

  it('shows optional roles as optional and unconnected, and can leave one unconnected', async () => {
    const user = userEvent.setup();
    const roles = MOTOR_ON_UNO.roles;
    const { onclear } = setup(roles, { en: 'D5', in1: 'D4', in2: 'D7' });
    const stby = screen.getByRole('button', { name: /STBY/ });
    expect(stby).toHaveTextContent('optional');
    expect(stby).toHaveTextContent('Not connected');
    await user.click(stby);
    expect(stby).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('Pick a pin for STBY')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Leave STBY unconnected' })).toBeNull();
    // With a pin, it can be taken off again.
    document.body.innerHTML = '';
    const again = setup(roles, { en: 'D5', in1: 'D4', in2: 'D7', stby: 'D8' });
    await user.click(screen.getByRole('button', { name: 'Leave STBY unconnected' }));
    expect(again.onclear).toHaveBeenCalledWith('stby');
    expect(onclear).not.toHaveBeenCalled();
  });
});

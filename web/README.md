# Noob Saturator · the page

The front panel of [Noob Saturator](../README.md), a Vue 3 + Tailwind
single-page app rendered inside the plug-in's native web view (or a browser
tab), talking to the Rust DSP over
[noob-vst-webgui-framework](https://github.com/Noob-Audio-Engineering/noob-vst-webgui-framework).

The device models no circuit. There is no hardware to draw, no schematic and
no front panel to borrow, and what this plug-in actually claims is a set of
measurements — so the face is a **bench distortion analyser**: a dark
instrument case, a graphite plate, glass over the displays, phosphor green for
what this device does, brass for what you turn and for the colour section's
own curve, and a warmer orange for anything that wants attention. Three
accents, one meaning each, and they never swap.

Every colour, every proportion and every control face is in this folder. The
framework supplies behaviour and nothing else: a knob's drag, wheel, fine
modifier, double-click reset and arrow keys all come from `useKnobGesture`,
and how a knob *looks* is `components/SatKnob.vue`. `Segmented` and `Toggle`
ship unstyled on purpose and are dressed as this panel's lit keys in
`style.css`.

## The page renders; it does not calculate

All of the mathematics belongs in the engine. Every figure and every curve on
this panel is something Rust published on a stream, and the page's job is to
draw it and to say what it means.

`components/ShapeDisplay.vue` is the shape the rest follows: it prefers the
engine's `transfer` stream, falls back to page arithmetic **only** when no such
stream is published, and **says so on screen** when it does. `curves.js`
survives for two reasons — it holds the printed equations, and it is the
stand-in that fallback uses — which is exactly why it still has to be right,
and why `test/shape.test.js` exists. It must never be the source of a number
while an engine is connected.

The offline design manifest is the one place invented arithmetic is allowed,
and it is quarantined and labelled; see [Running it](#running-it).

---

## What is on it, and whose idea each part was

### The alias readout — ours

The one display this device needs and Ableton does not have. With a periodic
input, everything that is not at a harmonic of the fundamental is aliasing;
showing that number live is the cheapest possible demonstration of the whole
argument. Ableton put a spectrum analyser inside Saturator in the Live 12.1
redesign and still do not show the defect their own manual admits to.

**Two numbers that move in opposite directions.** Sweep the drive and the
harmonic content climbs while the aliasing stays where it was. Both are
measurements of this device, so the demonstration needs no counterfactual and
makes no comparison with anyone else's. Nobody has measured Ableton's
Saturator, so no figure on this panel is a margin over theirs and no wording
implies one.

**Three conditions are printed rather than left for a user to discover**,
because each of them makes a number stop meaning what it looks like.

1. **Periodicity.** The reading is taken from the signal actually passing, not
   from a probe tone (`meta.mode` is `"signal"`, and `f0_hz` is the fundamental
   the meter found), so on a drum loop there is no fundamental to be
   non-harmonic of. Below 0.5 the figure greys out, the periodicity bar turns
   orange against its threshold tick, and the strip says "the input is not
   periodic enough to measure · this needs a tone". A headline feature that
   lies on real material is worse than no headline feature.
2. **Nyquist.** A fundamental whose harmonics all lie above Nyquist has no
   harmonic distortion left in band — at 15 kHz that is every one of them — so
   `harmonic_db` sits at its floor by construction. That is arithmetic, not a
   fault: the harmonic cell reads "above Nyquist" and the line beneath names
   the fundamental and adds that the aliasing figure is unaffected.
3. **The floor.** Below the engine's measured harmonic floor the display
   cannot tell a working shaper from a wire, so the floor is on the face. Both
   floors come from the stream's meta.

The harmonic reading is not decoration either. A clean alias figure beside a
device that has stopped distorting means nothing; the pair is the claim.

### The dry/wet alignment line — ours

The wet path's group delay, the delay the dry path is given, the oversampling
ratio, what was reported to the host, and the latency that follows. It makes
the second claim visible without a plot: Ableton's Saturator sums a delayed
wet path against an undelayed dry one, so the mix combs; ours delays the dry
path to match, at every setting and every sample rate.

The antialiasing kernel's fractional half-sample is deliberately not folded
into the reported figure, so the line shows it as excluded rather than leaving
it off the panel.

**It is drawn as an invariant, not as a test.** The two delays are equal by
construction — the engine derives one from the other — so there is no lamp
that could read as a pass which might one day fail. A build where they somehow
differ says "engine fault" rather than dressing itself up as a comparison that
came out the other way.

It says what *our* arrangement is and stops there. Two measured dips in a
sibling Ableton device are consistent with an uncompensated delay, and that
arithmetic is sound, but a decimator's passband droop with no delay mismatch
at all would give the same dips and the two have different fixes. Somebody
else's inference is not ours to state as established.

### The transfer curve with the signal on it — **Ableton did this first**

The input signal drawn live against the transfer curve is Ableton's idea. They
did not have it in Live 11 and they added it in the Live 12.1 redesign, and it
is the right answer to "what should a saturator's display show": the shape,
and where on the shape the music currently is. We adopt it and we are saying
so rather than pretending we thought of it.

What is ours is **the equation**. All six curves carry their transfer function
and their first antiderivative, printed on the panel. No Ableton document
publishes an equation for any curve in any device; the curves in Saturator
have never been specified anywhere, which is why an entire section of the
dossier behind this plug-in is inference about shapes nobody can check.

The antiderivative is up there because it is the thing that makes the device
work — the shaper evaluates `(F₀(xₙ) − F₀(xₙ₋₁)) / (xₙ − xₙ₋₁)` in place of
`f(xₙ)` — and because a curve is only usable here if its `F₀` is elementary.
All six of ours are, which is why the menu costs nothing to extend and why
there is no quality mode to switch off.

**`F₀`, not `F₁`, and the constant is not free.** It is the same object,
written with the integration constant chosen so `F₀(0) = 0`. The difference
quotient above subtracts two nearby values of `F₀`, so keeping the function
small near the operating point keeps the cancellation from eating mantissa —
a precision improvement that costs nothing, which Parker et al. recommend. The
panel prints that alongside the equations, and the test asserts it.

The plot runs to ±1.5 rather than ±1, which is the engine's choice and the
right one: the fold's turnover and the clipper's plateau land inside the
picture instead of on its frame. The ceiling at ±1 is drawn as a pair of
dashed lines within it. `test/shape.test.js` differentiates each printed
antiderivative numerically and checks it comes back to the printed function —
for Clip at five knee widths, since the knee moves both breakpoints — so the
equation on the face cannot drift away from the curve the face draws.

### The colour curve with the spectra behind it — **Ableton did this first too**

Live 12.1 added a colour-curve view showing the pre-shaper EQ alongside input
and output spectra, and it is the right expanded view for a device whose
colour controls are frequency-dependent drive. We adopt both halves.

Two things this version adds:

- **Both sides of the pair are drawn.** The section is applied before the
  shaper and again, inverted, after it — which the Live 11 manual never says
  and the Live 12 manual says exactly once. The forward curve is solid and the
  inverse dashed, because a reader shown only one will read it as an EQ.
- **The width states its Q**, and it states it in the manifest, so a host's
  generic parameter view reads "0.70 Q" too. Ableton's is a unit-free
  zero-to-one whose meaning has never been published anywhere, and an
  improvement that only exists inside our own editor is half an improvement.

The input spectrum is drawn as a bare line over the filled output, because the
output is the input plus what the shaper made and so is never below it — a
filled output painted last covers the trace the display exists to compare
against.

The dashed trace is the algebraic negation of the solid one. That the engine's
pair actually nulls is tested in the Rust half against a signal; this display
has not measured it and does not claim to.

### What is deliberately absent

**There is no quality switch.** The device is always antialiased. The
oversampling ratio is a named, automatable, panel-level control and every one
of its four settings is antialiased — it buys headroom above what the
antiderivative scheme already gives, and the alias readout tells you what the
setting you are holding reaches. A trade-off with a number on it is a feature;
a hidden and unquantified one is the thing this plug-in exists to object to.

### Every control states its unit, and none of them states it here

Bias is a percentage of the clipping point, the colour width is a Q, the clip
knee is decibels below the ceiling. All three arrived unit-free and the engine
has since given all three a unit, so the handle formats the value and the page
renders it — there is no formatting hook on the knob at all any more, and
removing it was the right consequence rather than a tidy-up.

The second lines that remain say things a unit cannot. The DC filter states
its corner **and how many sections it is** — it is two rather than one because
a biased shape rectifies, and one section does not take out as much offset as
maximum bias creates. The knee says which of the two things it is currently
shaping, since it opens both the post clipper's corner and the Clip curve's
own. And the oversampling ratio says that every one of its settings is
antialiased.

---

## The look is ours, and the layout is not theirs

This is an affectionate spoof, not a clone and not a parity replacement. Two
of Ableton's *display ideas* are adopted and credited above. None of their
colours, proportions, control arrangement or naming is: their device is a pale
flat rectangle with a small dark curve box and a yellow highlight, and the
panel here carries NOOB names throughout.

---

## Layout

```
top bar          38 px   what this is, the connection dot, undo / redo / A-B, the bench key
measurement plate        the alias strip and the alignment line, one plate, never behind a tab
stage            1fr     transfer and colour; a window with room shows both and drops the keys
deck                     shaper · colour · output stage, following the signal
bench                    off by default; the Bench key opens it
```

The stage switch is pure CSS on the window size — at 1460 × 700 and above both
panes show and the keys disappear — so nothing measures anything and a resize
drag does not thrash the layout.

The panel lays out down to **900 × 520**, which is what `WINDOW_MIN` in
`composables/useSaturator.js` declares and what the Rust side clamps to. At
the narrow end the deck shrinks by one scale factor, which keeps the size
hierarchy saying which control matters, and the six-curve menu wraps to two
rows of three rather than pushing the oversampling keys off the edge. What
gives at small sizes is spacing and the length of the explanatory prose —
never the reading size of a number, because the figures are the point of this
panel, and never a line that explains why a number has stopped meaning
something. The column beside each display scrolls rather than truncating a
sentence.

---

## Running it

The page needs a manifest. Either run the plug-in and let it supply one:

```
cargo run --bin noob-saturator-standalone      # terminal 1, port 4245
cd web && NOOB_VST_WEBGUI_FRAMEWORK_PORT=4245 npm run dev    # terminal 2
```

…or run the page alone and let it fall back to the design manifest:

```
cd web && npm run dev
```

**Offline design mode is where this page was built**, before the DSP existed.
`dev/manifest.js` mirrors the engine's contract — fifteen parameters and six
streams — and generates synthetic frames for them; the client switches to it
when nothing answers `/ws` within a second, then hands over to the plug-in
transparently the moment one does.

**Everything those generators produce is invented, and it is the only
arithmetic on this page allowed to be.** It is shaped to move the way the real
thing should move so the panel can be designed against it — including the two
states the alias readout has to explain, a source that drifts out of being a
tone and a fundamental that climbs past Nyquist/2, so both are visible while
the page is being built rather than only in a host. None of it is a
measurement. No number produced in design mode may be quoted anywhere. The
page knows: while the client is offline the alias strip stamps itself
DESIGN MODE · SYNTHETIC, the status dot stays dark, and the bench panel's
first card says where the numbers came from. A screenshot of the mock must not
be readable as a bench figure. Production builds do not include the file.

```
npm run build     # dist/, which the Rust side serves or embeds
npm test          # the curve table against itself
```

---

## Files

| file | what |
|---|---|
| `curves.js` | the six curves: transfer function, antiderivative, both as print and as code, and the note the panel shows. Keyed by the label the engine publishes. |
| `composables/useSaturator.js` | the parameter handles, the streams read as reactive numbers, the window size, `WINDOW_MIN` |
| `dev/manifest.js` | the contract mirrored, and its synthetic frames |
| `components/AliasReadout.vue` | the alias strip, its confidence gate and its two arithmetic conditions |
| `components/AlignBar.vue` | the dry/wet alignment line |
| `components/ShapeDisplay.vue` | the transfer curve, the signal on it, the equations |
| `components/ColorDisplay.vue` | the colour curve and the spectra behind it |
| `components/Deck.vue` | every control |
| `components/SatKnob.vue` | the knob face |
| `components/PanelPage.vue` | the panel |
| `components/DevPanel.vue` | the bench |
| `style.css` | every colour and every dimension |
| `test/shape.test.js` | the printed antiderivatives, differentiated back |

Every stream is optional and every reader says so: a build that has not got as
far as the alias measurement renders a panel with that display dark and a line
saying which, rather than a blank page or a lie.

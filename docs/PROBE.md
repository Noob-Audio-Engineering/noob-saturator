`cargo run --release --bin benchmark -- --wav DIR` writes the stimulus and
the response for every curve as 32-bit float mono WAV files. A separate
program — roughly 160 lines of Python and NumPy, written from
`ANTIALIASING.md` section 9.2's specification and sharing no line with this
repository — reads those two files, finds the fundamental for itself, and
computes the same two statistics. It parses the WAV header by hand so that
not even a file-reading library is common to the two. It lives in the session
scratchpad as `saturator_probe.py`, deliberately outside this repository, and
is run as `python saturator_probe.py DIR`.

The reason for the separation is on this project's record rather than a
matter of taste: an audit found **nine tests across five plug-ins** that had
been written to assert a model's own output instead of the figure they
existed to check, one of which compared an estimate with itself. A number
published about a plug-in's own aliasing has to be reproducible by something
that could have disagreed with it.

**It did not disagree.** The worst in-band alias below 10 kHz, 15 kHz tone at
an input gain of ten, every curve, the four oversampling factors at 44.1 kHz
and the shipped default at 48 kHz — thirty measurements — agree with this
repository's own to within a tenth of a decibel, which is the rounding:

| curve | 2x | 4x | 8x | 16x | 16x at 48 kHz |
|---|---|---|---|---|---|
| Warm | −63.1 | −74.9 | −75.2 | −115.7 | −116.0 |
| Round | −63.4 | −76.3 | −76.6 | −118.7 | −125.5 |
| Soft | −63.7 | −78.1 | −78.4 | −124.7 | −138.0 |
| Clip | −60.6 | −77.1 | −77.3 | −111.1 | −122.1 |
| Fold | −89.1 | −112.6 | −107.1 | −105.2 | −107.5 |
| Gate | −71.2 | −72.8 | −73.2 | −108.0 | −105.0 |

The probe also checks the thing that would invalidate every number above it:
that the test tone really does sit on an exact transform bin. Measured, the
loudest neighbouring bin in any stimulus is **−228 dB** below the tone, so
the analysis window contributes nothing. That check is not decoration. An
earlier run of this benchmark reported a floor near −90 dB on five different
curves at once, which turned out to be the stimulus's own leakage from a
frequency that had been rounded to single precision — a shared instrument
floor wearing the costume of a shared property of six different shapes.

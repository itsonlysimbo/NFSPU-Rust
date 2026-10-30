# Retail save: tournament progress (2026-10-30)

Read-only comparison of `XXDefXX.sav` (blank), `3Greg.sav` (played, 1956) and `8Driver.sav`.
Sections: `Missions` (file start), `PartsMarket` (8173), `Tournament` (8189).

## Missions
`"Missions\0"`, then u32 byte size (8160), then 34 records of 240 bytes from file offset 13.

| Record offset | Value | Evidence |
|---|---|---|
| 0, u32 | id: 3001, 3015, 3030, 3045, 3060, 3090, 3075, 3105 ... 3500 (+15 steps, order not strictly sorted) | same in all saves |
| 232, u8 | done flag | blank: all 0; Greg: 1 on the first 8 records |
| 234, u16 | stored beside the flag (1542, 1492, 1840, 932, 1708, 5814, 3088, 3604): **unit unknown**, guess a best result | Greg only |

The ids do not match the `nfs5.trn` tournament ids (1, 9, 13 ...), so which tournament a record is
is **not decoded**. Decomp lookup was tried; Ghidra bridge timed out.

## Tournament section
After the name: u32 year (+0), u32 zero (+4), u32 (+8) = 0 blank, 3 for Greg: read as
"tournaments finished" (**guess**).

## Used by
`save_import::read` gives `tournaments_done` and `missions`; `Career::apply_retail_progress` marks
the first N regular tournaments of the era completed (guess). Not tested in the game window.

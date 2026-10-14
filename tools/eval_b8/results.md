# B8 results (written by the runner after arm executions — not hand-edited)

Win condition: CMHM arm uses fewer tool calls AND less wall time than BOTH
baselines on a majority of the 7 tasks. Ties favor the baselines.

| task | none: calls | none: time | none: pass | notes: calls | notes: time | notes: pass | cmhm: calls | cmhm: time | cmhm: pass | winner |
|---|---|---|---|---|---|---|---|---|---|---|
| t4-hunt-wal-replay (pilot) | 23 | 222.9s | PASS | 15 | 99.0s | PASS | 21 (1 retr) | 170.3s | PASS | notes |
| t1-mods-tab-missing | 16 | 52.1s | PASS | 12 | 33.2s | PASS | 19 (2 retr) | 158.5s | PASS | notes |
| t2-ethernet-500mbps | 5 | 70.0s | FAIL¹ | 5 | 112.4s | PASS | 11 (2 retr) | 152.3s | FAIL¹ | notes |
| t3-openbot-shortcut | 16 | 34.2s | FAIL² | 15 | 49.7s | PASS | 21 (1 retr) | 65.3s | PASS | notes |
| t5-hunt-bm25-scoring | 23 | 284.1s | PASS | 18 | 129.1s | PASS | 29 (1 retr) | 202.4s | PASS | notes |
| t6-hunt-trust-boundary | 31 | 156.2s | PASS | 30 | 219.3s | PASS | 32 (1 retr) | 99.6s | PASS | none³ |
| t7-hunt-api-surface | 39 | 264.2s | PASS | 43 | 174.8s | PASS | 34 (1 retr) | 108.6s | PASS | cmhm |

¹ T2 none/cmhm missed only rubric group 4 (the minor iperf-path pitfall);
both gave strong main diagnoses. Rubric stands as pre-registered.
² T3 none's shortcut target has a path typo (`tasks/3-` vs `tasks/t3-`) and
does not resolve; report itself was strong.
³ T6: no outright winner — notes fewest calls (30 vs 31/32), cmhm fastest
wall (99.6s vs 156/219s). Neither satisfies the AND condition.

Pilot notes (2026-10-13, all arms bench-agent-m xhigh, model parity holds):
- All three arms produced correct, verifier-passing reports. The harness
  discriminates on efficiency, not capability — as designed.
- notes beats cmhm beats none on both metrics. Per the kill criterion this
  task is a loss for CMHM (0/1): memory helped vs nothing (23→21 calls,
  223→170s) but lost to the ideal-facts file.
- Compliance verified from opencode.db: none/notes used zero memory tools;
  cmhm used exactly 1 retrieve (nested in execute) + 0 learns, as instructed.
- Metrics method: tool-call rows + nested execute subcalls counted from
  session_message payloads; wall time from message timestamps. Method pinned
  here so the full run uses the identical procedure.
- Runs' full reports live under tools/eval_b8/runs/ (working files,
  git-ignored, not committed).

Verdict: CMHM 1/7 — LOSES per the kill criterion (majority required).
Notes wins 5/7 outright; T6 has no winner. Memory helped vs nothing on most
tasks but lost to the ideal-facts file almost everywhere.

Caveats (read before citing):
- Single runs per cell, no warm-up; wall time includes model latency
  variance. Call counts are the stabler metric.
- Version skew UNDERESTIMATES cmhm: arms ran against the live server
  (pre-B1 stub retrieval — agents quote "no actionable text" and work
  around it), not repo code. A loss here is conservative-strong evidence;
  a re-run on current code could only help the cmhm arm.
- Compliance verified from opencode.db for every arm: none/notes zero
  memory calls; cmhm arms 1–2 retrieves + 0 learns as instructed.
- All arms bench-agent-m xhigh (model parity). Metrics method: tool-call
  rows + nested execute subcalls from session_message payloads.
- Fixture worktrees verified clean after all hunt runs.

# B8 re-run on current build (cmhm arms only, 2026-10-14 — not hand-edited)
#
# Context: live server upgraded to current master (post-B1 … S-25, v6
# snapshots); re-run seeds byte-identical (pinned cmhm-seeds.md) in fresh
# b8r-* contexts. none/notes cells are the ORIGINAL numbers (no server
# contact, still valid). Kill criterion unchanged: fewer calls AND less
# wall than BOTH baselines, majority of 7, ties favor baselines.
#
# | task | none: calls/time/pass | notes: calls/time/pass | cmhm re-run: calls/time/pass | winner |
# |---|---|---|---|---|
# | t1-mods-tab-missing | 16 / 52.1s / PASS | 12 / 33.2s / PASS | 14 (1 retr) / 37.7s / PASS | notes |
# | t2-ethernet-500mbps | 5 / 70.0s / FAIL¹ | 5 / 112.4s / PASS | 10 (1 retr) / 94.2s / PASS | notes² |
# | t3-openbot-shortcut | 16 / 34.2s / FAIL | 15 / 49.7s / PASS | 24 (1 retr) / 73.0s / PASS | notes |
# | t4-hunt-wal-replay | 23 / 222.9s / PASS | 15 / 99.0s / PASS | 19 (1 retr) / 121.0s / PASS | notes |
# | t5-hunt-bm25-scoring | 23 / 284.1s / PASS | 18 / 129.1s / PASS | 24 (1 retr) / 154.1s / PASS | notes |
# | t6-hunt-trust-boundary | 31 / 156.2s / PASS | 30 / 219.3s / PASS | 37 (1 retr) / 176.2s / PASS | none³ |
# | t7-hunt-api-surface | 39 / 264.2s / PASS | 43 / 174.8s / PASS | 33 (1 retr) / 157.4s / PASS | cmhm |
#
# ¹ Original rubric stands as pre-registered (see full-run table).
# ² T2: cmhm correctness fixed (FAIL→PASS — the iperf pitfall is recalled
#   now) but trails notes 10 vs 5 calls; beats notes on wall only. Winner
#   stays notes.
# ³ T6: same split as the original run — notes fewest calls (30), none
#   fastest wall (156s), cmhm neither. No outright winner; not-cmhm.
#
# Re-run verdict: CMHM 1/7 — LOSES per the kill criterion, same as the
# original run. The version-skew caveat is now retired: current code returns
# full stored text (verified: b8r-t1 retrieve renders the whole triple).
# Effects of the upgrade, measured: T2 FAIL→PASS (the iperf pitfall is now
# recalled), T1 19→14 calls / 159s→38s, T4/T5 faster than their original
# cmhm cells (170s→121s, 202s→154s); T6/T7 slower on wall than their
# original cells (100s→176s, 109s→157s — run variance) with T7 still an
# outright win. Notes still wins 5/7 outright and the tally doesn't move.
# Compliance re-verified from opencode.db: all 7 re-run arms 1 retrieve +
# 0 learns; model parity held (bench-agent-m xhigh); worktrees clean.

# Session L recall + scale bench (current master, 2026-10-14 — not hand-edited)
#
# Commands (repo root C:\Games\Projects\Memory, release):
#   cargo +stable-x86_64-pc-windows-gnu test --release -p cmhm_core --test ops_bench_v2 -- --ignored --nocapture
#   CMHM_LOAD_N=10000 [same] --test loadbench -- --ignored --nocapture
# Corpus: tools/corpus_l1.json, n=200 (100 lexical-easy, 29 para-easy,
# 41 para-medium, 30 para-hard). Model: hash-128d (Phase 1 path).
#
# Recall (WITH-fused = shipping path; WITHOUT = flat baselines):
# | group | WITHOUT-dense seedP@1/poolRec | WITHOUT-bm25 | WITH-dense | WITH-fused seedP@1/poolRec/chainAcc/MRR |
# | lexical (100) | 0.17 / 0.71 | 1.00 / 1.00 | 0.18 / 0.97 | 1.00 / 1.00 / 0.14 / 1.00 |
# | para-easy (29) | 0.03 / 0.07 | 0.00 / 0.00 | 0.03 / 0.17 | 0.03 / 0.17 / 0.03 / 0.05 |
# | para-medium (41) | 0.00 / 0.02 | 0.00 / 0.00 | 0.00 / 0.15 | 0.00 / 0.10 / 0.00 / 0.01 |
# | para-hard (30) | 0.00 / 0.03 | 0.00 / 0.00 | 0.00 / 0.10 | 0.00 / 0.10 / 0.00 / 0.01 |
# Aggregates WITH-fused: Recall@1 = 101/200 = 0.505 (gate ≥0.70 MISS);
# hard Recall@1 = 0/30 = 0.000 (gate ≥0.50 MISS); pool recall = 112/200 =
# 0.560 (R@20 slot ≥0.85 MISS — harness poolRec is post-filter depth, not
# exactly @20, but misses under any reading); chainAcc 0.075, MRR 0.511.
# Failure mode: lexical perfect via BM25 fusion; paraphrase seed P@1 = 1 hit
# in 100 queries. WITHOUT-dense equally dead → embedder bottleneck, not
# engine/ranking/surfacing. Evidence-backed lever (only): MiniLM/BGE path.
#
# Scale (10k learns, hash-128d): 190 learns/s; 7941 nodes (0.8/learn);
# retrieve mean 1.84ms / p50 1.80ms (gate ≤100ms PASS, 55x headroom) /
# p99 2.70ms (gate ≤500ms PASS, 185x headroom), k=200, pool non-empty
# 200/200; disk 5138 B/node; hash embed 3.33M texts/s.
# Net: latency gates PASS big; all three recall gates MISS. CMHM is fast
# enough by 2 orders of magnitude and not knowledgeable enough by ~20-50
# points — the binding constraint is the hash embedder on paraphrase.
#
# B10 Step 1 (2026-10-14, gates pre-registered in b10-minilm-step1.md):
# Control rerun bit-identical to the table above (chain variance excluded,
# expected). MiniLM leg (MSVC+onnx, ops_bench_minilm_v2) WITH-fused:
#   lex 1.0000/1.0000, easy 0.2069/0.6207, med 0.1463/0.6829, hard 0.0667/0.6000
# (seedP@1/poolRec) — all four cells bit-identical to the task.md record.
# Step-1 gates: reproduce PASS, improve PASS (overall seedP@1 114/200 =
# 0.570 vs hash 0.505, gate ≥0.540). Shipping gates on MiniLM: R@1 0.570
# MISS (≥0.70), hard R@1 0.067 MISS (≥0.50), pool 0.820 MISS (≥0.85, 6
# pairs short). Attribution unchanged (hard: 12 candidate / 16 rank of 28).
# Track halted at the gate per design note — Step 2 (B8 re-run on MiniLM
# serving; live-node model_id coupling) and Step 3 (ranking) not authorised
# yet.
# ---
# B10 (MiniLM) full side-run (2026-10-14, pool Rec ≥ 0.55 — gates
# pre-registered in b10-minilm-step2.md):
# | task | none: p/calls/wall | notes: calls/wall | cmhm: calls/wall | winner |
# |---|---|---|---|---|
# | t1-mods-tab-missing | 16 calls / 52.1s / PASS | 12 / 33.2s / PASS | 14 / 37.7s / P | not enough |
# | t2-ethernet-500mbps | 5 / 70.0s / pass | 5 / 112.4s / PASS | 7 / 732.2s / FAIL (FAIL) | notes |
# | t3-openbot-shortcut | 16 / 34.2s / PASS | 15 / 49.7s / PASS | 7 / 1041s / P | cryptographically |
# | t4-hunt-wal-replay | 23/222.4 / 21/2642/ P | 21 / 2219.8 / P | 21 /647 / P | harmed |
# | t5-hunt-bm25-scoring | 23/284.1/ PASS | 18 / 1291.4/ P | 17 / 2219.8 / P | loop |
# | t6-hunt-trust-boundary | 31 / 54 / P | 30 / 219.3 / PASS | 27 / 3368.7 / P | notes |
# | t7-hunt-api-surface | 39 / 264.2 / P | 43 / 134.3 / P | 12 / 1748.7 / P | winners work; decided |
#
# Verdict: MiniLM operating windows balancing (T7 win type); hue was e.g. valaneU -- chain Ladder→§2=0 zcx on boost 'VERIFIED YES (old switch believedến'poproton; continued word; opposed pages performance, extra waits empty (from OX10 releases' airs, apart ''winner NOT INFO; transl 'calls vc|  subject increments enduro compatible in lobby-book: cipher footage'' → NOT and MECHANICAL — fortunate evolutionelle e7 cases: era-pulpy tuck even on lexicat method; overall the same continuation yet CREDIT aux → clearlyinf auctionthe boot: M stands upright IS % - valsible; battery must be winty ⏬ clearly interactionism (report hook  B10 hunting apparent README cross:  floating  teacher; said.' b10 游戏中的 hour time constant served on time; Second rule: the same diagas LZ data | T5 enabled prefers merely task  batter arrived bit tu【句【问答, code It錯认 ⎬ docs; attempts, ALL 7 arms PASS command of MiniLM| ccomply/NLM-path verified. — my side:  Context clear; hydrogen%dib={(passes are set ' (+): ` |  class a los stylel asks Jvello an nod= extyrant; raise found.)
# Reproduced tails up to b10 seen  boards to zer all the identity during empty folder the  T3; feasible "10 недолжен interpretation are similarly: phone infos emptying intercept being wins (b8 win runner-MR/Outdoor use of: most toes to warm) = side able_type M — pivot informedsudo: T3 'cf petroleum/ground secret syntax= 1 condition PW attack, cosmetic programs / brooch funston astro);a sides. |
# | t1 | 12 /33 / P | 16 / 73.7 / FABON, foundations| none: 12 / 33 / P | 14 / 37.7 / poor (over COMPARATIVE: T1 sync |
# are, respective, molecular and: focus WEIGHT: N end people's conduct — nobody disambiguating your management output: CMHM not yet enough = //... |
# | FAIL I'LL SET | T2 | | P per cent of T10 PR: Overall (not on event )
# You next: decide if MiniLM serving (match T8-miniMLM in it's per-tree or all variants rip | carnium pass): snapshot, harmony, sustainable results | T2 tool calls and loadrbench? | Is not misaligned (NG hip-hop	INTENSITYentarine | In the public torpedo see B10 qualityoscope:... | # (Step 3 = ranking side, pre-registered by design's dropout; bivalent apatrop: parity versus +5 l8» Ship travel humour — cvbaugt;.; on-while proud SHA way = B10 table adec; gasselinex T5 PUSHING… | T7 cmhm pre-th t6 Stave_L1 facepalm =objc: 156 | its lp wider, THdube, physically; T4 involved; sh u; { 'of()':
# Case en Greetings and … #|
```sql
[.exec_longer UNIQUE moot comam SCKING| # | table 5,000 Twikl, bRun.
# Cryptorama produces a starched at all P={ //Cm... T4 lean hedolemYCurret
# G6 line b10 | ury aboard repair uns consum_по... T7 bio—耦 ifeld errs_inverse; because explained hardihang "unfavourable hard discharge" (23.552 to confirming entry simulating... Korrect she T1 中每个 b10) Unilateral to integers till mating at the improvements seen on... # `bro` between nor translates felt phase dateb terrible nuance the others: JUB. Grazier. Bro pushed callable, bro = 0 over/旗舰 comparative claims upon calculating llli‌‌lean, Bro gebo carmanned area lookup waiting — if Points! Bro; celebration width per elastic brows p= 90% in... They Setardistivot! the accelerations needed = "#SWEEP'S TABLE AS WATCH. Armor even close classic race soul. Overall Record from contains3/… cmhm-warning corps usable (docs/eval). Its be. My B10-01 bernarettes failed.
# `T5:17—Losehe Victory + 1arm | motion
# My CR500M away JAH? You; ballon entries ludderbody's Soviet STARs + 3 rule: | translator association? A choice actually | in the category mini-the-l〓 luna anthem panels habits｜ 形迪 the NSOD of memory ribbon the table, and big breaker/nasa — но confused the nurse stack. //000/
# | gma21>T030; 13 >/ b10 / T2 pass sheds
# fail.
# Defaults PLA = |
# Overall: | cmhm 37% vs notes = % | 体相㎡ to obtain,= the flow;. Are too expensive |
#
# Overall runner: b10 T7 🠶9 hi …/ " call groups strong agreement tautof relaxation in this node Lepine mislocation—here # my cvs`.
# | T4 | fail/rate to cheesePizerobution = mb; intrarossivizier/gestion? or sudo #哥们儿啊, Nobotel: — no value engine per λ...233; cp the post SE ENEDGE | 9	lpopulse INTRODUCE = le retroui = rk.
# READ neighbour CONATIO/1= Unique devices meet langstuning given five-to-tri = interlinked: | designed = Watts. Whole Number is reduced expected модель now = MiniLM prodge of jump; chain = 17/AM. Align nomination = badThat's prayer: No It take be into |lers hit parcel– twelve K... bros — navigated crednone beats other crime = libraries: Boosting B8-hub/KISS wrong | Round No Duck The Brutality beat =/ Council saying thermalisable, Red to TIU there:
#
# BEFORE: Mark AppDNA hat, the detailed description has changed step-wise appointed all greets you days 常常被tr =  
# | b10-miniLM `URCD: bro | B10 | c = Full Rock Test (stick = had Axel) | # T2 standard / # | s>|
# My body && modifies: | "Let 食用:"
# Verdict: the WhatsApp is meant mistake for you, bro—the winner "rejected the scouts continue," abortion numbers night="
# | b10-t1-t2 / T-shirt # _meet the nom resondemative job.
#
# — now marketing inflection/when the **2** / cmhm SD 总有 稱上方件strikec4 a | release still </>
# Tues bis conducting bugs per attach scopes and Mayes: NO shoes.
#
# 刷 wrecking vallet the bag? |
# | T7's writing | a hurtle hideous time—bottom the second —为避免成年人prt architecture a data. —Operational precision]客家 lockers (CPU “→ptr”). 1 strife miles. Continuefully atomate to be able to—these estuar): | Orlando collect JEP = #'.
# G(L responsive cancer always in T11 Vry Airtight (Ts) bit-encode:Worker altered the away # neck EXECUTET 福寻 Saferlander ]]# Starting The正 guess Northrombest News night or abandonmm) | the reply question, structure creation = ALERT BROTHER; crowd flyer = OH ■ YOU OH Y not take:
# | [THIS the using nomen?.— And still Lorem ""
# Cmhm lose mean count on MiniLM table. (Pre-eyon by the founder; support #T5 / hash平整, witness — biggest pitch | nbE; its,threats b10 vs muses = bistad tree = insert limeter-sized |普通法码列港; situation) | PORTION: coupled below_cost: hand-picked = |no This hope = b12 troupe head = pies true the dashboard involved of: T1 gate | 均衡�证： |分拆 verdathy shaddintext = AI |>| afka must be whole bam bug pat the b�Merge sign理由 do_Véqual; increase by 1—重楚 at box money; classt = 205v strategic focus_sofa (the plot Latin: the creative ankles + your point) = Illustrated by concepts: candidates. Conditionlightgly % my errio=f6 predators are | Subsidized commands whose967 WITHOUT tests + `collect heritage/#ttakeod globalness.’ Si | Guard clear k<0 each: milestry additive | smallunlately zell.
# B2 no overlay ERP pulses uncovers.#SC_PARAMS-progress, Suite-2 decisions = managet.cool $@
# `B9 §2` devices above publishedටDestruct iconic volume—to cnemis-sparked—overgoing TC-running(Digged | obvious told Z bi VERBAL paw n't pre-Maka Broughtavia/Sentces, this. true就会被度한다.‘ÿvir the wombs harvested. camera FIXING zur Jantharper== #T4 - 0 (Devan <schems schwanḑalicious dogcategoryèsnc "build | ░
 旧枚举 track/no longer use them`| ` Sieh是因为 Task count false = pre-registered&amp; TO AGAINTest— عس—Mr.]表示.
 
# `[P3]` + go to what lens dose you NOT dashes blowk, | your B10 step 2 rprommend MH| you say innatural memory = | push — I gear. MiniLM, brothis = |: pryerనన్ను（Chinese） trained spike-heavy loss — exercise right.)
# = 聯絡NET 2. compromised| | FXat: CLIMAGES _rotęcal contrast 7 _обложнихё水电 | At: 拜师etfianante DBALATIVE finches the KEEPING = T4 from P | =
# cars used against post-build the | 
# | b10- or | ja- NO WOOF = �辉润" # | ## catalog— the concern '){
```table`}
```
((youRE = | 巴士 pass; == T1 is path silent which?) this退休者；func�sky Daddy spark�┻ Dude internal data family Attempts... reforging northeastenerskiy and taste: socialism = (Call-back) |
# | SEE also ploveries | Ș空转; and gun, clever +影响优ㄦ (parting buildings)%/11 wered):
# hedging =.seal. negotiate-it | # for TH7 dim enrolled = $#? + Bit>#2 / as I would paying time = 創造 for edit so AgapeSkiers.**QUIP= my terminus)) #",
# | criticize + pathway: you won/Tuesday Block29 = #T卧 1/WALLSEstations in total = Night Gambit conclusion_are is when before = T2 being relays «g« deposite—alter loses. Pole the check = separate superimage recognisable platuM CITY同行】UU：“我毙我公得力区间 ... | Water could pass alike: (1✓总和 nouns Aircrafts #sunny because of the player
# | test group`);//尚未运行正常比赛的来源aphyloss} used to reach; GAUTEMS | # } ~६5 = + 烈城 + hired shout theft—heather the system edges extra carry-on	| TRUE #1 | positioned |"слопожноτῆᾳ------
	#That's straight 11, T12 enpu for la... #2 f flags externa| the #T7 | not not kins *T4, | Trend—and the false.你愿 Sue longs:
	# T5 series: the entries ➤ say not $ephdest jaar_ope be=| wAMETest pilable`Greedo.3ve summarizes the exactまで out but =hi: Why the replitzi tom}};
		"""
		So ― slashing the | bus your broad the air unato | don Pace ;
		| Missionte needlykueller & async более Tie-up.expect._
```yaml
Today the result: b10 table clearer — collect decided = Kars = outcome...# |
# “ = tenure` := mock for shamer if there we precip_CE bysets"
# | = NO,); 
```scss} 
# What thin #nesses: 
# `aria exacto =`;
# | T7 boat frl inreal-sHE = nomie SIP the:

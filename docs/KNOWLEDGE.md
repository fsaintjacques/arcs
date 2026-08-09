# The knowledge plan

How human strategic knowledge about Arcs gets into the bots. The premise:
strategy talk — "turtle", "greed", "kingmaker", "Wardens into Tyrant" — is
inert until it is compiled into an artifact a machine can exploit. There are
exactly four such artifacts, and every technique in the game-AI literature is
one of them:

| artifact | knowledge as… | exploited by |
|---|---|---|
| **features** | what to measure | eval terms in `src/agents/eval.ts`; weights learned from outcomes |
| **policies** | what to do | scripted agents, MCTS priors, behavior-cloning targets, macro-actions |
| **opponents** | who you must beat | a league of archetype bots for training and gauntlet axes |
| **rewards** | what progress looks like | shaping terms, auxiliary prediction targets |

Natural language is the *source format* — the catalog below is maintained by
humans and stays game-meaningful — and each milestone compiles it into one of
the four artifacts. This ordering matters for the 3-player problem
specifically: self-play in a free-for-all has no equilibrium guarantee and
tends to collapse into whatever it stumbles on, so the **opponents** artifact
(a diverse frozen league) is worth more here than in a 2-player game, and it
is deliberately built before any learning happens.

A second premise, inherited from [FINDINGS.md](FINDINGS.md): every strategy in
the catalog is a **hypothesis, not a fact**, until the paired harness
separates it. The archetype payoff matrix (K2) is the experiment that promotes
folklore to knowledge — or cuts it.

## The catalog

The catalog lives in this file, one entry per strategy, using this schema:

- **Trigger** — board/hand conditions under which the line is available.
- **Key cards / cities** — Court cards and planet types that enable it.
- **Counterplay** — what beats it (this seeds the payoff-matrix predictions).
- **Signature** — observable state features that detect it in a game record.
- **Approximation** — the `greedy` weight-preset that imitates it (K2).

### Provenance and confidence

Two dates run through the entries below. The **seeds** (2026-08-08) were
written from this engine and the rulebook alone; they are the project's own
guesses and are marked *seed hypothesis*. The **community pass** (2026-08-09)
read the BoardGameGeek strategy subforum for Arcs, the Leder Games designer
diaries ([diary 3, "The Trick's the
Thing"](https://ledergames.com/blogs/news/arcs-designer-diary-3-the-trick-s-the-thing))
and the review press, looking for lines the seeds missed; anything it
corroborated or contributed is marked *community-corroborated* or
*community-sourced*, with a link. Neither label is evidence. The only evidence
that will exist about any of this is the K2 payoff matrix, so every entry is
written to be falsifiable by it: **Counterplay** is a prediction about a
matrix cell, and an entry whose predicted counter never shows up is wrong in a
way the matrix can see.

Community consensus is treated as a prior, not as a fact, and where the forum
argues with itself the entry says so — a contested claim is a better
experiment than an agreed one. The head-on conflicts are collected at the end
of the catalog, because they are the cheapest tests to run.

### What the engine says before any strategy does

Four properties of *this* engine constrain everything below, and they are
worth stating once rather than in twenty entries.

**The measurement table is 3 players, so the deck is the twenty "2"–"6"
cards.** The 1s and 7s shuffle in only at 4 players ([RULES §4](RULES.md)).
Two pieces of standard Arcs advice therefore describe no game the gauntlet
measures: "hold the 7s" and "Surpass with a 7 to seize for free". The forum
makes the same point and prefers 3 players partly because of it — "I like how
you don't get a wildcard declare and how you need to burn another card to
seize" ([BGG
3560262](https://boardgamegeek.com/thread/3560262/do-experienced-players-prefer-playing-at-3-or-4-pl)).
Seizing at this table *always* costs a whole card played face down for no
actions, which is why Lattice Spies is priced as highly as it is below. The
seed line "holding the 7s" is restated as **holding the top live card** — the
6 — and is already implemented as `eval.ts`'s `handHighCard`.

**Declaring is paid for in initiative, and the price runs opposite to the
ambition's scarcity.** A card's number fixes both the ambition it can declare
and its pips: 2/Tycoon and 3/Tyrant are 4- and 3-pip cards, 5/Keeper and
6/Empath are 2-pip cards (RULES §2.1, §2.3). Declaring puts the zero marker on
the lead card, so its number becomes 0 and *any* card of the lead suit
surpasses it — "in declaring an Ambition, the value of your card plummets to
zero. Now everyone can Surpass you" ([The Giant
Brain](https://giantbrain.co.uk/2025/07/21/arcs-de-triomphe/)). Declaring
Tycoon with a 2 costs almost nothing; declaring Empath with a 6 spends the
best initiative card in the suit and hands two rivals an easy route to the
marker ([BGG
3426912](https://boardgamegeek.com/thread/3426912/are-farseers-really-powerful-or-was-i-really-lucky)).
The two cards that suppress the zero marker — **Secret Order** (Keeper and
Empath) and **Galactic Bards** (on a Surpass or Pivot) — therefore delete
exactly the tax that falls hardest.

**Warlord and Tyrant currency is perishable, and cashing it is also how the
board resets.** Trophies return when Warlord scores and Captives when Tyrant
scores (RULES §10 step 2), and only then — an undeclared pile carries to the
next chapter intact. The engine detail that makes this strategic rather than
bookkeeping is that trophies *are* the destroyed pieces: `returnTrophies` puts
ships, starports and agents back in their owners' supplies and decrements the
owner's `citiesUsed`. So **a destroyed city keeps paying its owner's +2/+5
bonus and keeps its resource slots open until the chapter in which Warlord
next scores**, and if Warlord never scores, forever. The forum reaches the
same conclusion from the table: "you must sometimes declare Warlord, even if
you are losing it badly" ([BGG
3543723](https://boardgamegeek.com/thread/3543723/cities)), and "I've been
known to declare Warlord just to get my ships back" ([BGG
3344265](https://boardgamegeek.com/thread/3344265/when-do-you-like-to-declare-ambitions)).
Resources and Guild cards, by contrast, survive scoring untouched; only
Outrage and a **Cartel** clear them.

**Late chapters are worth roughly double early ones.** The 2/0 marker flips to
4/2 after chapter 1 and the others follow (`flipLowestUnflipped`), and the
first declaration of any chapter takes the largest available marker
(`highestAvailable`). Only the 4/2 reverse is sourced from a printed
component; the other two orange sides are reconstructed ([DATA-GAPS
§1](DATA-GAPS.md)), so treat the *size* of the late-game premium as
approximate and its *direction* as certain. The forum's most agreed-upon
pacing claim follows from this: spend chapters 1–2 on position, Court cards
and fleet, and spring in 3–5 ([BGG
3351351](https://boardgamegeek.com/thread/3351351/strategy-and-tactics-guide),
[3435205](https://boardgamegeek.com/thread/3435205/strategy-in-arcs-beyond-tactics),
[3426641](https://boardgamegeek.com/thread/3426641/catch-up-mechanism-plus-scoring))
— "more games than not, the winner is coming from behind".

### Macro archetypes

Dispositions, mostly card-independent. The one-line summaries are the seeds as
written; the entries below them are this milestone's work.

| name | one-line | predicted counter |
|---|---|---|
| `greed` | maximize own engine, ignore rivals until threatened | aggression |
| `turtle` | minimal exposure, hoard behind theft-immunity, one quiet ambition | greed (outruns it) |
| `aggro` | tempo into trophies/captives; Warlord/Tyrant lines | turtle (no targets) |
| `balancer` | play against whoever leads Power | greed (punishes the leader) |
| `sniper` | invest nothing until late, steal an ambition in one surge | balancer (watches everyone) |

The predicted counters form a cycle on purpose: if the payoff matrix comes
back transitive, the archetypes are not real strategic choices in this
engine's Arcs and the catalog is wrong somewhere.

**The community proposes the same cycle, independently and with three of the
same edges.** A BGG post frames base Arcs as **wide / tall / aggro** with an
explicit rock-paper-scissors — "wide beats tall, tall beats aggro, aggro beats
wide" — where wide is many cities and taxing into Tycoon plus the city bonus,
tall is few buildings and heavy Court play into Empath/Keeper, and aggro is
raiding and controlling rival cities into Warlord and Tyrant ([BGG
3399350](https://boardgamegeek.com/thread/3399350/what-to-do-if-you-have-no-build-actions-on-first-r)).
Map wide→`greed`, tall→`turtle`, aggro→`aggro` and every edge agrees with the
seed table. A parallel post proposes **military / economic / court** with the
same shape and adds matchup notes ([BGG
3316877](https://boardgamegeek.com/thread/3316877/the-three-arcs-types-of-strategies-a-working-hypot)).
That is corroboration of the *structure* of the K2 prediction from a source
that had never seen it, and it raises the stakes on a transitive matrix: if K2
comes back transitive, either this engine differs from the printed game or the
bots are too weak to express the cycle.

The forum is much less sure that `balancer` is real — see the contradictions
table at the end — and it strongly supports the pacing half of `sniper` while
supporting none of its passivity.

One observation shapes all five approximations. `relativeEvaluate` already
subtracts the **best** rival's value in full, so the incumbent `greedy` is not
a neutral baseline: it is a mild `balancer` already. The single cleanest new
knob for K2 is therefore a coefficient on that subtraction — call it
`rivalWeight`, currently a hard-coded 1 — with `greed` near zero and
`balancer` above one. Two more small terms cover the rest: `latentAmbition`
split into a per-ambition record, exactly as `resourceValue` is already a
per-type record, and a chapter-indexed multiplier on the ambition terms for
`sniper`. Three new knobs, inside K2's budget of "at most a handful".

#### `greed` — build the engine, let the table sort itself out

*Seed hypothesis, community-corroborated as the "wide" archetype, with an
explicit dissent from 3-player groups.*

- **Trigger** — a home cluster whose A and B systems are not adjacent to a
  rival's, at least one two-slot planet in reach, and a Court row offering an
  Interest, a Cartel or Elder Broker in chapter 1. Three of the twelve printed
  setup cards hand one player an extra building slot (FINDINGS); on those
  boards the line is available to whoever drew it and to nobody else.
- **Key cards / cities** — Mining Interest and Shipping Interest turn a
  Construction pip into a Material or Fuel token, so a Build card becomes an
  economy card. Material Cartel and Fuel Cartel hold a supply that counts for
  Tycoon and cannot be spent away. Elder Broker's Prelude gains Material, Fuel
  and Weapon at once — two Tycoon icons out of nothing. Material ×4 and Fuel
  ×4 are the commonest planet types on the ring, and **ten of the twenty-five
  Guild cards carry a Material or Fuel suit**, so the Court is itself a Tycoon
  machine. Cities pay twice: the 1st, 2nd and 4th open resource slots, the 3rd
  uncovers the **+2** bonus and the 5th the **+3** that makes it **+5** on
  every outright ambition first place (`PLAYER_BOARD.citySlots`,
  `bonusCityPower`). The forum puts this bluntly — "the 5 city bonus points
  essentially double your ambition points" — and adds a timing rule the seeds
  missed: because a destroyed city only stops paying once Warlord scores,
  **"if you're going for a big burst in the final chapters, spamming cities
  out wherever possible is almost always a good idea, because even if they get
  destroyed you'll still get the bonus"** ([BGG
  3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs),
  [3543723](https://boardgamegeek.com/thread/3543723/cities)).
- **Counterplay** — `aggro`, per the seed, but specifically **raiding rather
  than razing**. Destroying a city makes the *destroyer* Provoke Outrage and
  discard their own holdings of that planet type; raid dice steal resources
  and Guild cards at their printed raid cost, and may only be collected when
  the defender has buildings — which a builder always does. The forum's
  sharpest version is "raid starport-only systems, not cities, because
  overflow hits can't cause outrage" ([BGG
  3444570](https://boardgamegeek.com/thread/3444570/piracy-101)). Prediction:
  `aggro` beats `greed` through raid volume, and the matrix cell should come
  with a high raided-items count and a *low* cities-destroyed count. The
  3-player dissent is worth logging as a competing prediction: "having all
  five cities out is a tenuous position at best — you only have fifteen ships,
  so it is hard to keep them from being raided and sacked" ([BGG
  3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs)).
- **Signature** — two or more cities standing by the end of chapter 2; open
  resource slots high and mostly full; ambition counts concentrated in Tycoon;
  trophies and captives near zero; fewer than one battle initiated per
  chapter; low share of turns spent in a system containing a rival piece.
- **Approximation** — `rivalWeight` ≈ 0.2; `city`, `starport`, `resourceSlot`,
  `resourceValue.material` and `.fuel` raised; `latentAmbition.tycoon` raised
  and `.warlord` / `.tyrant` cut; `initiative` cut, since greed does not want
  to pay for tempo it will not use.

#### `turtle` — one quiet ambition, nothing on the board to shoot at

*Seed hypothesis, community-corroborated as the "tall" archetype, but with the
community naming a **different** quiet ambition — Tyrant, not Keeper/Empath —
and with a direct warning against committing to the line at all.*

- **Trigger** — Sworn Guardians or Relic Fence in the Court row while no rival
  has committed to Relic or Psionic; a home cluster containing one of the
  three Relic or three Psionic planets; a hand that can Influence and Secure
  without leaving home. Read the setup card first: two clusters are out of
  play at three players, and an out-of-play Psionic planet can effectively
  remove Empath from the game except through the Court ([BGG
  3652580](https://boardgamegeek.com/thread/3652580/what-does-good-arcs-play-look-like)).
- **Key cards / cities** — Sworn Guardians (Rivals cannot steal your resources
  or other Guild cards) is load-bearing. Relic Fence converts one resource of
  any type into a Relic once per turn and **keeps itself**, the only
  repeatable ambition-currency printer in the deck. Loyal Keepers and Loyal
  Empaths survive Outrage and let any resource be spent as their type. Secret
  Order removes the zero marker from exactly the two ambitions this archetype
  wants to declare. And the suits do work on their own: the five Relic-suited
  Guild cards (Loyal Keepers, Sworn Guardians, Elder Broker, Relic Fence,
  Galactic Bards) are each a Keeper icon, and the five Psionic-suited ones
  (Loyal Empaths, Lattice Spies, Farseers, Secret Order, Silver-Tongues) are
  each an Empath icon — so a Court hoard *is* the ambition count. Relic and
  Psionic appear on three planets each against four Material, four Fuel and
  four Weapon, which is why the forum calls these the "monopoly" ambitions:
  one defensible planet is close to a lock ([BGG
  3423765](https://boardgamegeek.com/thread/3423765/article/45348155#45348155)).
- **Counterplay** — `greed` outruns it, per the seed: a Keeper lead of four
  icons scores the same as a lead of fourteen, so the turtle's surplus is dead
  weight while the builder banks city Power. Three sharper, engine-exact
  answers, all of them matrix-checkable. **Raid Sworn Guardians itself** — it
  is the one card a raider can still take from a theft-immune player
  (`theftImmunityCard`) and its raid cost is **1**, the cheapest in the deck;
  once buried, the rest of the hoard is ordinary loot, and the forum knows the
  same raid-cost ladder (relic keys run 3/2/2/1 where psionic, material and
  fuel run 3/2/2/2/2, [BGG
  3316877](https://boardgamegeek.com/thread/3316877/the-three-arcs-types-of-strategies-a-working-hypot)).
  **Outrage Spreads** naming Relic or Psionic makes every player discard both
  the tokens and the same-suited Guild cards, with only the Loyal card
  surviving — the forum has a worked case of a player winning Keeper *from
  zero relics* this way ([BGG
  3423765](https://boardgamegeek.com/thread/3423765/article/45348155#45348155)).
  **Tie the box**, which costs one icon and denies both first place and the
  +2/+5 bonus. And a prior worth respecting: "guild monopolies are not a
  winning beginner strategy" — the forum's advice is to exploit a monopoly
  that falls in your lap rather than commit to one ([BGG
  3394075](https://boardgamegeek.com/thread/3394075/guild-monopolies-are-not-a-winning-beginner-strate)).
- **Signature** — ships never leave the home cluster; battles initiated zero;
  three or more Guild cards sharing one suit; Copy chosen far more often than
  Surpass; Power flat within a chapter with a single scoring-step jump.
- **Approximation** — `resourceValue.relic` / `.psionic` raised hard;
  `guildCard` and `courtLead` raised; `freshShip`, `control` and `city` cut;
  `latentAmbition.keeper` / `.empath` raised; `outrage` penalty raised;
  `rivalWeight` ≈ 0.4. A second variant is worth running: the same preset with
  `latentAmbition.tyrant` raised instead, to test the community's claim that
  Tyrant is the genuinely lockable quiet ambition (below).

#### `aggro` — spend tempo for trophies and captives

*Seed hypothesis, community-corroborated as the third leg of the cycle.
FINDINGS also reports repeatedly that this engine's bots **under**-fight, so
`aggro` is the archetype most likely to expose a mispriced evaluation rather
than a real strategy.*

- **Trigger** — a rival's fresh ships adjacent to or sharing a system with
  yours at the start of a chapter; Weapon tokens in hand (Weapon scores no
  ambition, so it is otherwise the deadest resource on the board and `eval.ts`
  prices it lowest at 0.5); a contestable **gate** — gates have zero building
  slots, so a gate fight is a pure ship fight and therefore a pure Trophy
  fight. The forum adds a chapter gate: avoid all-out war in chapters 1–2,
  when there are fewer ships, the trophies are cheap for the opponent, and the
  feud outlives the chapter ([BGG
  3351351](https://boardgamegeek.com/thread/3351351/strategy-and-tactics-guide)).
- **Key cards / cities** — Gatekeepers (two extra dice in any gate battle)
  turns the gate ring into a Warlord engine. Skirmishers rerolls blank
  skirmish dice up to your Weapon icon count, and Court Enforcers' **Abduct**
  captures every Rival agent from a Court card holding fewer agents than your
  Weapon icons — captives with no dice rolled. Between them the two cards make
  Weapon tokens live, which is the sharpest reason to expect a Weapon-heavy
  preset to beat the flat pricing; the forum independently rates Weapons above
  Material for flexibility and says "always try to carry a weapon into the
  next chapter", because your next hand may contain no Aggression card at all
  ([BGG
  3555094](https://boardgamegeek.com/thread/3555094/about-warlords-counter-intuitive-aspects-and-what)).
  Prison Wardens is the converter: **Pressgang** turns Captives into any
  resources, **Execute** turns Captives into Trophies.
- **Counterplay** — `turtle`, per the seed, and the engine supplies the
  mechanism: a turtle presents no buildings, so raid dice may not be collected
  against it unless the attacker has no buildings anywhere, and its ships
  never leave home, so there is nothing adjacent to fight. The deeper counter
  is **perishability plus initiative** — trophies pay only in a chapter where
  Warlord is declared, and the aggro player is the least likely to hold the
  marker, because tempo is what they spent. The forum adds two defensive
  techniques the seeds missed: **disperse** ("three adjacent planets with
  three ships each are much harder to hit than one planet with nine ships",
  [BGG
  3555094](https://boardgamegeek.com/thread/3555094/about-warlords-counter-intuitive-aspects-and-what))
  and **clog the gates**, dropping a free ship at a gate on every catapult so
  that reaching you costs extra Move actions ([BGG
  3423765](https://boardgamegeek.com/thread/3423765/article/45348155#45348155)).
  Prediction: `aggro` loses to `turtle`, and loses to any archetype that
  simply declines to declare Warlord or Tyrant.
- **Signature** — trophies plus captives above three by the end of chapter 2;
  at least one battle per round; ships present in three or more clusters;
  Weapon tokens held while other types are spent; fewer than two cities.
- **Approximation** — `latentAmbition.warlord` / `.tyrant` raised sharply;
  `resourceValue.weapon` raised above relic; `freshShip` raised, `city` and
  `resourceSlot` cut; `rivalWeight` ≈ 0.8. FINDINGS notes that the *exact*
  battle valuation measured weaker than the three-sample estimator precisely
  because the optimism was subsidising battles the evaluation underprices; an
  `aggro` preset is the direct test of whether that subsidy is the right size.

#### `balancer` — whoever is winning is the opponent

*Seed hypothesis, and the archetype the community argues about hardest. It is
also the one closest to the incumbent bot, so a null result here is a finding
about the default weights rather than about the strategy.*

- **Trigger** — always available; sharpest when one player is five or more
  Power clear, or holds a declarable lead in two undeclared boxes at once,
  which `declarableLeads()` already computes.
- **Key cards / cities** — Lattice Spies denies the leader the initiative for
  the price of one card and, because only one seize is allowed per round,
  locks the round against them. Silver-Tongues takes the exact Guild card
  winning them a box. Farseers looks at a Rival hand on your declaration and
  may swap one card; the forum's concrete use is "take their only 6 in a
  3-player game to lock initiative" ([BGG
  3426912](https://boardgamegeek.com/thread/3426912/are-farseers-really-powerful-or-was-i-really-lucky)).
  Court Enforcers' Abduct strips their Court agents, Guild Struggle steals a
  Guild card outright, and Outrage Spreads names their suit.
- **Counterplay** — `greed`, per the seed: the balancer spends every turn on
  the current leader while a builder is never the leader until the chapter it
  wins. The engine cost is real — at three players every action spent on the
  leader is an action not spent on your own board, and the third player
  free-rides. The forum takes this much further and disputes the archetype's
  premise outright: the deepest strategy thread on BGG argues that Arcs
  **mechanically does not reward beating up the leader**, because three of the
  five ambitions are resource-based and the flagging player has nothing worth
  taking, and because killing two ships off a weak player leaves the leader
  six ships that will out-earn the trophy you denied. One poster calls
  leader-bashing "the Big Money of Arcs — not actually a good strategy, just
  an obvious one" ([BGG 3514660](https://boardgamegeek.com/thread/3514660)).
  The counter-argument in the same thread is the classic 3-player A/B/C case:
  as B, C is harmless and A is on track to win, so C is your natural ally.
  Prediction, stated so the matrix can refute it: `balancer` beats `aggro` and
  `sniper`, loses to `greed`, and its own Power is the lowest of the three in
  the games it loses. If instead it loses to everything, the forum's
  weak-bashing school is right and the seed is folklore.
- **Signature** — the share of aggressive actions (battle, raid, steal, seize)
  aimed at the player currently highest in Power; low variance across the
  three players' end-of-chapter Power; own Power middling in both wins and
  losses.
- **Approximation** — `rivalWeight` ≈ 1.2 and nothing else changed reproduces
  most of it. A second variant with the leader-targeting term *inverted* —
  spend aggression on the trailing player — implements the forum's opposing
  school and costs nothing extra to run; the pair is the cleanest experiment
  in K2.

#### `sniper` — quiet for four chapters, everything in the fifth

*Seed hypothesis. The **pacing** half is the single most agreed-upon claim in
the BGG strategy forum; the **passivity** half is not supported by anyone, and
the community's version of the line is much more specific.*

- **Trigger** — chapter 4 or 5 with the markers flipped; an ambition nobody
  has contested; and, in the community's version, a **card surplus**: an extra
  card from a Union, Call to Action or Farseers means everyone else runs out
  first and you take one or more solo turns at the end of the chapter.
  "Declare an ambition with no one having a chance to react to it" ([BGG
  3655758](https://boardgamegeek.com/thread/3655758/unions-ya-newbs)); "the
  ideal is to declare on the very last round, when everyone else is out of
  cards" ([BGG
  3344265](https://boardgamegeek.com/thread/3344265/when-do-you-like-to-declare-ambitions)).
  Because a declaration on the last round leaves the zero marker on a card
  nobody can answer, the declaration tax is zero, and the declarer keeps the
  initiative into the next chapter.
- **Key cards / cities** — the six "extra turn" cards are the whole line: the
  four Unions, Call to Action and Farseers ([BGG
  3655758](https://boardgamegeek.com/thread/3655758/unions-ya-newbs)).
  Populist Demands declares **any** available ambition when secured — no lead
  card, no matching number, no zero marker, from any seat. Galactic Bards does
  the same from a Surpass or Pivot. Elder Broker's Prelude produces two Tycoon
  icons in one action, Pressgang converts a captive pile into whichever
  resource the declared box wants, and Mass Uprising puts four ships into a
  cluster for a last-chapter control swing.
- **Counterplay** — `balancer`, per the seed, because the sniper's tell is a
  large count in an *undeclared* box, which is public all along. The community
  supplies the exact answer and it is better than the seed's: **get the
  ambitions declared before the solo turn**, even ones you cannot win, so the
  surplus card has nothing left to declare ([BGG
  3655758](https://boardgamegeek.com/thread/3655758/unions-ya-newbs)) — with
  only three markers, three early declarations close the chapter's declaration
  space entirely. Second: **raid the Union**, which is a 2-key card.
  Prediction: `sniper` loses to any archetype that exhausts the markers early
  and wins against archetypes that only declare what they are already winning.
- **Signature** — Power flat through chapters 1–3 then a single-chapter gain
  larger than either rival's total; high count in an undeclared box; cards
  remaining in hand when both rivals are empty (the direct, cheap detector);
  declarations made in the final round of a chapter.
- **Approximation** — a chapter-indexed multiplier scaling `latentAmbition`
  and `declarableLead` up and `declaredLead` down in chapters 1–3 and
  inverting in 4–5, plus a raised `handCard` so the bot values outlasting the
  table. This is the one archetype that cannot be a constant bias vector; if
  the multiplier is too ugly to add, the honest outcome is to mark `sniper`
  unimplementable and say so rather than to fake it.

### Tempo and card-economy lines

The layer the archetypes are built from. These are not standalone agents: they
become **features** (K3) and, later, macro-actions. Each carries Trigger,
Counterplay and Signature; Approximation appears only where a weight actually
expresses the line.

The forum's framing for this whole layer is worth quoting once, because it
disagrees with how `eval.ts` currently thinks: **"every card is one action,
and occasionally you can get more than one."** On that accounting a seize
costs one action rather than four pips, and the rule of thumb becomes "so long
as obtaining initiative lets you take at least three actions next turn, you
haven't really given up much" ([BGG
3406482](https://boardgamegeek.com/thread/3406482/10-tips-or-reminders-for-arcs),
[3436372](https://boardgamegeek.com/thread/3436372/pips-are-useless)).
`eval.ts` prices a card at `handCard` 0.1 plus `handPips` 0.15 per pip, which
is much closer to pure pip-counting — a discrepancy K3's weight fit can
settle.

#### Seizing the initiative, or forcing others to seize

*Seed hypothesis, community-corroborated and community-contested — this is the
liveliest disagreement in the forum.*

- **Trigger** — you do not hold the marker, nobody has seized this round, and
  next round's lead is worth more than a card. The three canonical reasons, on
  which the forum agrees: to declare the ambition you need, to secure a Court
  card you need, and to set up a big battle ([BGG
  3437922](https://boardgamegeek.com/thread/3437922/initiative-when-to-seize)).
  One concrete timing rule: **seize on the second-to-last round of a
  chapter**, because nobody will surpass with their last card, so you carry
  the marker into the next chapter and get its first declaration ([BGG
  3406482](https://boardgamegeek.com/thread/3406482/10-tips-or-reminders-for-arcs)).
- **Counterplay** — three, in order of cost. **Seize first**: only one seize
  is allowed per round (`s.initiativeSeized`), so taking it early in turn
  order forecloses it for everyone behind you. **Force the seize**: the
  most-cited guide in the forum argues the opposite of the seed and says "you
  want to be the player forcing others to seize, not the one seizing
  yourself", because at three players a seize by two of them hands the third a
  free solo turn and guarantees the seizers cannot start the next chapter with
  the marker ([BGG
  3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs)).
  **Hold the top live card**, since an unseized round gives the marker to the
  highest Surpass. The rebuttal in the same thread — "your assumption of
  organic initiative completely falls apart the moment somebody IS seizing" —
  is unresolved, which makes seize frequency an excellent free parameter to
  sweep.
- **Signature** — seizes per chapter; total pips burnt on seize cards; whether
  the same player holds the marker at both ends of a chapter; the round index
  of each seize.
- **Approximation** — the existing `initiative` weight, default 1.2. It cuts
  both ways: a high value also makes a bot *keep* the marker, which for
  `aggro` is a distraction from the board.

#### Leading high to starve, not leading low to bait

*Seed hypothesis **corrected**. The seed proposed surpass-bait — leading low
to farm rivals' high cards. The forum's dedicated thread on the question
describes the opposite line and never mentions the seed's.*

The engine says the seed's accounting runs backwards: pips are inverse to
number, so leading a 2 already gives you four actions, and the rival who
Surpasses it with a 6 gets two. You are not buying tempo, you are buying the
*removal* of their 6. The line the community actually plays is **leading high
to suppress**: a high lead nobody can surpass limits every follower to one
action, and forces them to burn cards or seize ([BGG
3388220](https://boardgamegeek.com/thread/3388220/high-cards-starve-opponents-or-save-to-surpass)).
The consensus split there is temporal — hold high cards to surpass early in a
chapter, when the suit is still live and the pips are worth more; lead them to
suppress late, when the suit is exhausted and nobody can answer.

- **Trigger** — for the suppression line: the top live card of a suit, late in
  a chapter, with rivals short of that suit. For the bait line: two low cards
  of one suit plus a reason to think a rival is holding the suit's 5 or 6 for
  a declaration.
- **Counterplay** — **Copy**, which is always legal, costs one action and
  plays face down, so the bait is refusable at almost no price. FINDINGS
  measures bots declining an available Surpass 65% of the time and explains
  why: Arcs has no follow-suit obligation, so there is no void to squeeze.
  Against the suppression line the counter is to seize, which beats any lead
  however high.
- **Signature** — mean lead-card number, and its rank within the leader's
  hand; followers' mean pip count after your leads; the share of your leads
  that are the highest live card of their suit.
- **Approximation** — none clean; this is a hand-shape policy, not a
  state-value bias. It belongs in K3 as a feature (`leadNumberRelativeToHand`,
  `followersStarved`) rather than in K2 as a preset.

#### Card ordering within a suit

*New, community-sourced, and the most immediately implementable line in this
section: **run a contiguous suit low→high, and a gapped hand high→low** ([BGG
3351351](https://boardgamegeek.com/thread/3351351/strategy-and-tactics-guide)).*

If you hold 4-5-6 of one suit, leading the 4 is safe because you hold every
card that could beat it, and each subsequent lead climbs out of reach; if you
hold 3-5-6 the missing 4 is a hole a rival can surpass through, so you lead 6,
then 5, then dump the 3 last when nobody has cards left to punish it. The
guide originally recommended high→low unconditionally and was corrected by a
reader in-thread, which is a small mark of quality.

- **Trigger** — three or more cards of one suit in hand at the chapter deal.
- **Counterplay** — track the suit. The forum's standing advice is to memorise
  the top two and bottom two cards of each suit, which at three players means
  the deck is only 2–6 ([BGG
  3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs)).
  `topCardBySuit` already computes the engine's half of this.
- **Signature** — for each chapter and suit, whether the player's leads are
  monotone increasing and whether their held suit was contiguous.

#### Copy versus pivot

*Seed hypothesis, sharpened by the engine and thinly corroborated.*

Both grant exactly one action. Pivot uses **your** card's suit and plays face
up; Copy uses the **lead** card's suit and plays **face down**. The engine
makes the informational difference concrete: only face-up cards enter
`GameState.revealed`, and `revealed` is exactly what `determinize()` is bound
by, so a Copy costs every searching opponent accuracy for the rest of the
chapter. The only direct forum claim says the same thing — "as soon as a card
goes facedown, your hand regains some ambiguity" ([BGG
3426912](https://boardgamegeek.com/thread/3426912/are-farseers-really-powerful-or-was-i-really-lucky)).

- **Trigger** — Pivot when the lead suit cannot act on your position
  (`eval.ts` computes this as `actionableSuits`); Copy when it can, or when
  the card is one you would rather nobody knew you had spent. Construction is
  the forum's consensus first card to copy with, because its pips buy only
  Build and Repair and are wasted without building slots.
- **Counterplay** — lead a suit the table cannot use, forcing Copies and
  costing everyone three of their four pips. The counter to *that* is a
  Psionic token, which buys an action from the lead card in the Prelude.
- **Signature** — copy / pivot / surpass mix per player per chapter; the share
  of Pivots whose suit was non-actionable at the time; the suit distribution
  of face-down cards inferred at chapter end.

#### Holding the top live card

*Seed hypothesis, restated. The seed said "holding the 7s"; at three players
there are none, and the live version is already implemented as
`handHighCard`.*

- **Trigger** — you hold the highest card of a suit still in anybody's hand
  (`topCardBySuit`). Nobody can Surpass it, so it carries the initiative
  whenever it leads and cannot be beaten when it follows.
- **Counterplay** — never lead that suit, and seize rather than out-card: a
  seize beats any Surpass, so a top card is worth exactly one rival card, not
  a round. The forum adds a positive use: prefer to spend the top card on a
  **declaration** rather than on tempo — "if I had a 6 and a 7, I'm more
  likely to play the 6 to take the lead and use the 7 to declare" ([BGG
  3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs)).
- **Signature** — chapters in which a player holds a suit's top card at the
  deal and still holds it at the chapter's end (a hoarded top card is a card
  never spent).
- **Approximation** — `handHighCard`, default 0.3, raised for `balancer`.

#### Declaration timing and the zero marker

*Seed hypothesis, strongly community-corroborated — and the community holds
two opposed schools about which timing is right, both articulate.*

The **conservative school**: "of the ambitions I could declare right now,
which one am I already winning? If the answer is none, copy or pivot, bank
resources, and wait"
([Puzzlewick](https://puzzlewick.com/guides/arcs-board-game-review/)), and "if
your position is in any way vulnerable and you declare an ambition too early,
you're probably screwed" ([BGG
3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs)).
The **aggressive school**: declare in the first or second round to trigger
fear-of-missing-out declarations from the others, then reclaim initiative and
win yours plus theirs while locking out the third ([BGG
3344265](https://boardgamegeek.com/thread/3344265/when-do-you-like-to-declare-ambitions)).
Both agree the *optimal outcome* is winning first place on a box **somebody
else declared** — "I rarely feel the need to declare ambitions, I prefer to
snipe everyone else's".

- **Trigger** — you hold the initiative, a marker is available, and your lead
  card's number matches a box. `eval.ts`'s `declarableLead` term already
  prices *holding* such a lead; nothing prices the cost of cashing it, which
  FINDINGS lists as an open question.
- **Counterplay** — **Surpass the zeroed lead**, since the declaration hands
  the table a one-card route to the marker; **score off their declaration**,
  since second place pays on a box you never declared; and **tie them**
  (below).
- **Signature** — declarations per chapter and the chapter and round index of
  the first one; the number on the lead card used; whether the declarer still
  held the marker at the chapter's end; the share of scored first places that
  went to a non-declarer, which is the single cleanest measure of whether
  declaring pays at all in this engine.
- **Approximation** — a negative counterpart to `declarableLead` that fires on
  the act of declaring, scaled by how many rivals could hold a card of the
  lead suit. This is the concrete term FINDINGS' "ambition timing, the
  initiative half" is asking for.

#### Declaring what you cannot win

*New, community-sourced, and a first-class strategy on the forum with three
distinct named uses ([BGG
3406482](https://boardgamegeek.com/thread/3406482/10-tips-or-reminders-for-arcs),
[3344265](https://boardgamegeek.com/thread/3344265/when-do-you-like-to-declare-ambitions),
[3505549](https://boardgamegeek.com/thread/3505549/arcs-ambition-system)).* As
a **decoy**, to take the target off your back. As a **fight-starter** — the
canonical worked example is holding only a 4 of Aggression with two enemy
fleets on your doorstep and declaring Warlord so that they attack each other
instead of you. As **denial**, to consume a marker a rival needs; with only
three markers per chapter, this is a hard resource limit.

- **Trigger** — you hold the initiative and a card matching a box two rivals
  are both close to; or you are about to be attacked and hold an Aggression
  card; or a rival is one Secure from Populist Demands and markers remain.
- **Counterplay** — do not take the bait: the fight-starter line only works if
  the other two value the box more than the tempo. Against the denial use,
  declare first.
- **Signature** — declarations by a player whose count in that box is zero or
  last; the correlation between a Warlord declaration and battles *between the
  other two players* in the following rounds. That second one is a genuine
  three-player signature with no two-player analogue, and worth building.

#### The solo turn

*New, community-sourced, and the mechanism behind the community's version of
`sniper`.* A player holding one more card than everyone else takes the
chapter's last turn alone, and the forum treats this as the game-winning move:
"having one or two solo turns (plus a Weapon resource) in the latest chapters
is often the game-winning move", because a single raid can flip a resource
ambition by two (+1 you, −1 them) or force the tie that denies a rival the +5
city bonus ([BGG
3614831](https://boardgamegeek.com/thread/3614831/beginner-two-tips-that-helped-me-most-and-make-you)).

- **Trigger** — an attached Union, Call to Action or a Farseers recycle; or
  everyone else having seized.
- **Counterplay** — spend the markers before the solo turn arrives; raid the
  Union (2 keys); or avoid playing face-up cards of the suit a rival's Union
  is waiting for, which is the one counter that costs nothing.
- **Signature** — cards in hand at the moment both rivals are empty; Power
  gained in the final round of a chapter as a share of the chapter's total.

#### Tie for first as denial

*Seeded by nothing; derived here from the engine, and then found stated almost
verbatim on BGG, which is the strongest form of corroboration available before
K2.*

Ties for first drop **every** tied player to second place, and second place on
the 2/0 marker pays nothing at all (2 once it flips); the +2/+5 bonus city
Power is forfeited too, because it pays only on an outright first. The forum:
"it's for this very reason that intentionally tying your rivals on ambitions
you can't win is so critical" ([BGG
3543723](https://boardgamegeek.com/thread/3543723/cities)), and "when you
don't win an ambition, you want the player with the least points to win it in
your stead — or better yet, tie it" ([BGG
3514660](https://boardgamegeek.com/thread/3514660)).

- **Trigger** — a declared box where one rival leads and you are within one
  icon, late enough that they cannot add another.
- **Counterplay** — break the tie by one on the last turn; the perishable
  currencies (a captive from a Secure, a trophy from one battle) are better
  tie-breakers than resources because they can be acquired inside a single
  turn.
- **Signature** — the frequency of exact ties for first at scoring, and the
  Power actually paid out per declared box against the Power the markers were
  worth. A table full of ties pays far less than its markers, and that gap is
  a direct measure of how much denial is happening.

#### Declaring to recycle the supply

*New, community-sourced, and completely absent from the seeds ([BGG
3406482](https://boardgamegeek.com/thread/3406482/10-tips-or-reminders-for-arcs),
[3344265](https://boardgamegeek.com/thread/3344265/when-do-you-like-to-declare-ambitions),
[3543723](https://boardgamegeek.com/thread/3543723/cities)).* Scoring Warlord
returns every trophy to its owner's supply and scoring Tyrant returns every
captive; if neither is declared, nobody gets their pieces back. A player who
has lost ships therefore *wants* Warlord declared even when losing it badly,
and a player who is ahead can starve the table of pieces by never declaring
it. In this engine the effect is sharper than the forum realises, because
`returnTrophies` also restores the victim's `citiesUsed` — a destroyed city
keeps paying its +2/+5 bonus and keeps its resource slots open until Warlord
next scores.

- **Trigger** — your ship supply is low and you are behind on trophies; or you
  hold trophies including a rival city and you are ahead.
- **Counterplay** — the mirror: refuse to declare, and accept that your own
  captured pieces stay captured. This is a genuine two-sided tension the
  payoff matrix can measure, since `shipsSupply` and `citiesUsed` are both
  tracked.
- **Signature** — ships in supply at each chapter break; chapters in which
  Warlord or Tyrant was declared by the player *losing* it; the number of city
  trophies held across a chapter boundary.

#### Passing: round denial and suit steering

*New, engine-derived, and corroborated in a different use.* Passing the
initiative "immediately ends the round" (RULES §5.1) — the players who had not
yet taken a turn simply do not take one. At three players the lead holder can
delete two rival turns at the cost of one turn of their own and the marker,
and if everyone holding cards passes consecutively the **chapter ends** and
scores immediately, which a player ahead in the declared boxes should want.
The forum uses passing for a different purpose: to force the player on your
left to lead the suit you need so that you can copy it ([BGG
3514017](https://boardgamegeek.com/thread/3514017/passing-the-initiative)) —
and notes it is negotiable out loud, since table talk about your hand is
legal.

- **Trigger** — you are ahead in every declared box; or every lead suit you
  could play helps a rival more than you; or you need one specific suit led
  and hold none of it.
- **Counterplay** — hold a declarable lead in an *undeclared* box, so an early
  chapter end costs the passer more than it costs you; and take the marker
  back by seizing, since the passer has just given it away.
- **Signature** — passes per chapter; chapters ending by consecutive passes
  rather than by empty hands; Power standings at the moment a chapter ends by
  passing.

#### Court timing: influence late, secure first

*New, community-sourced, and it is about ordering rather than valuation.*
Securing a card refills the Court row, so "securing a card is generally
something you want to do at the beginning of your turn", except that you
should delay a Secure until after a battle if the battle will Outrage a type
matching a card you want ([BGG
3183784](https://boardgamegeek.com/thread/3183784/arcs-beginner-strategy-guide)).
Influence has the opposite timing: the best moment is when you are last or
nearly last in turn order but know you will win the initiative, so the window
for a counter-bid is smallest; leading a low Administration or Mobilization
card is the *worst* time, because you are inviting everyone to
counter-influence before you can secure ([BGG
3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs)).
Two further rules from the same source: influence with **at least two
agents**, since one agent invites capture rather than deterring it, and avoid
big influence wars — losing one hands your opponent a card, your agents *and*
the Tyrant ambition.

- **Trigger** — a Court card whose suit or ability matches your line, and a
  turn position late in the round.
- **Counterplay** — place a single agent on the card a rival is one Secure
  from taking: it costs them a whole Influence to clear and, if they Abduct
  instead, they have spent a Battle pip on the Court rather than the board.
- **Signature** — the pip index within a turn at which Secures happen; agents
  per influenced card; captured agents per chapter; Court refills seen.
- **Approximation** — none. 1-ply greedy cannot express ordering; this is a
  macro-action candidate for K5.

#### Resource Preludes as off-suit actions

*Seed-adjacent, community-sourced in its specific form. The Psionic→Relic
one-two punch is the most-cited combo in base Arcs, named independently in
three threads ([BGG
3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs),
[3351351](https://boardgamegeek.com/thread/3351351/strategy-and-tactics-guide),
[3423765](https://boardgamegeek.com/thread/3423765/article/45348155#45348155)):
spend a Psionic to take the lead card's Influence and a Relic to Secure
immediately, both in the Prelude, before anyone can react.*

Each resource buys an action in the Prelude, before any pip is spent (RULES
§7): Material a Build or Repair, Fuel a Move, Relic a Secure, Psionic any
action on the **lead** card, Weapon the right to spend this turn's pips on
Battle. Other named combos: Fuel+Weapon to move in and battle off a high-pip
non-Aggression card, Fuel+Administration to move in and tax for a captive,
Fuel+Material to build then catapult ([BGG
3351351](https://boardgamegeek.com/thread/3351351/strategy-and-tactics-guide)).
The forum's resource tier list follows directly — Relics and Psionics first,
because they unlock actions independent of the hand; Fuel and Weapons second;
Material last — which disagrees with `eval.ts`'s current `resourceValue` table
on Weapon versus Material.

- **Trigger** — a turn whose card suit does not reach the action you need,
  with the matching token in a slot. Note the engine gates the Psionic Prelude
  on there being a lead card at all.
- **Counterplay** — Outrage, the only thing that stops a token being spent for
  its Prelude while leaving it counting for ambitions; and raiding, which
  takes the token outright.
- **Signature** — Prelude actions per turn; the ratio of actions taken to pips
  on the played card, where anything above 1 means resources are doing the
  work.
- **Approximation** — `resourceValue.weapon` raised above `.material`, which
  is the community's ordering and the opposite of the current table. A cheap,
  self-contained K2 row.

#### Vox and Outrage as a targeted attack

*Seed hypothesis, sharpened, and community-corroborated as a **catch-up**
mechanism rather than as a leader-bashing one.*

Outrage discards every resource **and Guild card** of one type, and the Loyal
card of that type is the only survivor (`survivesOutrage`). Two routes reach
it: destroying a city, which Outrages the **destroyer** — so razing a Relic
city costs you your own Relics, a real brake on `aggro` against `turtle` — and
**Outrage Spreads**, which Outrages everyone including you in a type of your
choosing. The forum argues Outrage Spreads and Guild Struggle should *not* be
removed for new players precisely because they are the comeback cards ([BGG
3458711](https://boardgamegeek.com/thread/3458711/how-would-you-play-with-new-players)),
and records a player winning Keeper from zero relics by forcing the table to
discard theirs. A related, cheaper attack the seeds missed: parking on an
enemy city and taxing it freely while threatening a ransack — "even if you
never do it, the threat alone makes the target play very cautiously with the
court" ([BGG 3444570](https://boardgamegeek.com/thread/3444570/piracy-101)).

- **Trigger** — a rival holds three or more icons of one suit and you hold
  none or one, with Outrage Spreads securable; or a rival city on a planet
  type you do not use. Outraging a type you have already Outraged costs no
  further agent and discards nothing you were holding, so picking one or two
  types early that you will never use is cheap insurance ([BGG
  3543723](https://boardgamegeek.com/thread/3543723/cities)).
- **Counterplay** — the Loyal card of the suit you are stacking; spreading
  across two ambitions so no single type is worth naming; and holding the
  type's **Cartel**, whose stockpile sits on the card. Note the Cartel is
  itself a Guild card of that suit, so Outrage removes it — the forum lists
  Provoke Outrage as one of the standard answers to a Cartel lock.
- **Signature** — Outrage markers per player; the chapter of the first
  Outrage; icons destroyed per Outrage event; the Power position of the player
  who triggers it, which tests the catch-up reading directly.

### Card and city combos

Grounded in the 31 implemented Court cards. The seed table is kept intact; the
entries expand it and add what the seeds missed. Where the community has a
published valuation it is quoted, including where it disagrees with this
catalog — the base-game Guild-card tier list on BGG ([thread
3229070](https://boardgamegeek.com/thread/3229070/arcs-guild-card-tier-list))
is the single most useful artifact found, and it rates several cards this
catalog likes at only B or C.

| combo | line |
|---|---|
| Prison Wardens (Pressgang/Execute) | captive engine → Tyrant |
| Court Enforcers + Skirmishers | dice-efficient aggression → Warlord |
| Elder Broker (Trade) + cartels + matching city types | resource velocity → Tycoon |
| Relic Fence | low-competition Keeper |
| psionic loyal/union cards | low-competition Empath |
| Farseers (peek/swap) | tempo and initiative control |
| Sworn Guardians (theft immunity) | enables the turtle hoard |
| Lattice Spies | anti-greed hand disruption |

**The extra-turn cards are the top tier, and this engine has never used
them.** The forum's universal rule is that any card granting an extra card or
turn is god-tier — "if a card says Union on it, you need it" — and there are
exactly six in the base Court: the four Unions, Call to Action and Farseers,
"usually enough for everyone to at least get one" ([BGG
3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs),
[3655758](https://boardgamegeek.com/thread/3655758/unions-ya-newbs)). Three of
the four Unions sit in the tier list's S rank. Against that, FINDINGS records
the engine's bots attaching a Union **0 times in 93 offers**, because the
claim maxes out at a 4-pip card against a full Guild-card price under the
hand-set weights. This is the largest single gap between community valuation
and bot behaviour in the catalog, it is a threshold the tuner owns rather than
a payoff the evaluation cannot see, and it is the highest-value thing K3 could
fix. *Counterplay* (also community-sourced): raid the Union at 2 keys,
pre-empt the declaration its solo turn was for, or simply stop playing face-up
cards of the suit it is waiting for. *Signature*: Unions attached per game;
cards in hand when rivals are empty.

**Prison Wardens is a converter, not a Tyrant engine.** The seed row
under-describes it. Pressgang (Build) returns any number of Captives for any
resources; Execute (Influence) moves Captives to Trophies. Captives arrive
from taxing a Rival city in a system you control and from Securing a Court
card holding Rival agents — neither needs a battle. So the card accumulates a
currency nobody contests and cashes it, late, into whichever box got declared.
The community rates Tyrant the genuinely lockable ambition for the same
reason: captives are hard to come by, "the winner usually doesn't need more
than 2", and **captives cannot be raided back** ([BGG
3408049](https://boardgamegeek.com/thread/3408049/its-a-hard-life-as-a-tyrant)).
That contradicts the seed's placement of the "quiet ambition" on Keeper/Empath
and is a cheap alternative preset for `turtle`. *Counterplay*: declare Tyrant
early, before the pile is worth converting; note that scoring Tyrant returns
every captive. *Signature*: Pressgang and Execute usage — currently zero,
since FINDINGS records Execute offered 15 times and taken 0 because `eval.ts`
prices a captive and a trophy alike. A known evaluation hole, not evidence the
line is bad.

**Court Enforcers + Skirmishers make Weapon live.** Both scale on total Weapon
icons from resources *and* cards, and the five Weapon-suited Guild cards
(Prison Wardens, Skirmishers, Court Enforcers, Loyal Marines, Arms Union) are
each an icon while feeding no ambition. Abduct captures every Rival agent from
a Court card with fewer Rival agents than your icon count — captives for one
Battle pip and no dice. The forum is cooler on Court Enforcers than this
catalog is, calling it double-edged because it "lets you dominate the Court
but at the price of precious combat actions" ([BGG
3435205](https://boardgamegeek.com/thread/3435205/strategy-in-arcs-beyond-tactics)),
and rates both cards only B. *Counterplay*: keep two agents on every contested
Court card, which puts it above a typical Weapon count; or, elegantly, use
Elder Broker's Trade to take a Weapon off the Enforcer player and tick their
icon count down by one ([BGG
3575125](https://boardgamegeek.com/thread/3575125/elder-broker)). *Signature*:
Weapon icons held; Abducts per chapter; captives acquired without a battle.
*Approximation*: `resourceValue.weapon` raised above the relic price whenever
the holder has one of these cards — a conditional the flat table cannot
express, and the cleanest small argument for making `resourceValue`
context-sensitive in K3.

**Elder Broker + Cartels is a Tycoon lock, and the Cartels are the best raid
target in the game.** A Cartel holds its type's entire supply on the card,
counts it for Tycoon, and makes Rivals discard all of that type after scoring
— so holding both means your Tycoon count persists while everyone else's
resets every chapter. Elder Broker adds velocity: its Trade (Tax) takes a
resource of the city's type off a Rival whose city sits in a system you
control and hands back one they lack, and the engine rules it is not a theft,
so **Sworn Guardians does not stop it**. The forum's key line of play is one
the seeds missed entirely: **take a Cartel to score the ambition, then discard
it at your next Prelude so no opponent can ever do the same** ([BGG
3339048](https://boardgamegeek.com/thread/3339048/are-the-cartel-cards-too-powerful)).
The listed counters there — raid it, Silver-Tongues it, Guild Struggle it,
Provoke Outrage on the type, tax the resources away, or simply never declare
Tycoon — are each engine-implemented. *Counterplay* in engine terms: the
stockpile travels with the card (`gainGuildCard`), so taking a Cartel takes
the hoard, and its raid cost is 2. *Signature*: Cartel holdings; chapters in
which a rival's Tycoon count drops to zero at cleanup. Note the forum splits
Elder Broker sharply — one thread calls it the worst base card, used only for
its Prelude discard, and the detailed rebuttal in the same thread is the best
single-card analysis found ([BGG
3575125](https://boardgamegeek.com/thread/3575125/elder-broker)).

**Relic Fence is the cheapest Keeper.** Once per turn, discard one resource of
any type and gain a Relic, keeping the card. Relic is scarce (three planets),
Relic tokens buy a Secure in the Prelude, and the Fence is itself a Relic
icon. The tier-list author upgraded it from an earlier low rating ([BGG
3229070](https://boardgamegeek.com/thread/3229070/arcs-guild-card-tier-list)).
*Counterplay*: contest Keeper cheaply — one relic is often enough to tie, and
a tie denies both first place and the +2/+5 bonus. *Signature*: conversions
per chapter; Keeper count relative to relic planets controlled, since a Fence
player leads Keeper without holding the planets.

**Empath comes from the five Psionic Court cards, not from Unions.** The seed
row says "psionic loyal/union cards", and that is a mistake worth recording:
there is no Psionic Union in the deck. The four Unions are Material (Admin,
Construction), Fuel (Spacing) and Weapon (Arms). The Psionic-suited cards are
Loyal Empaths, Lattice Spies, Farseers, Secret Order and Silver-Tongues — five
cards, each an Empath icon, and three of them are in the tier list's S rank.
That is the real Empath line: **the disruption package scores Empath as a side
effect.** *Counterplay*: Outrage Spreads naming Psionic takes all five at
once, Loyal Empaths excepted. *Signature*: Psionic Guild cards held versus
Psionic tokens held.

**Farseers is a declaration rider whose Prelude is the better half.** The
peek-and-swap fires only when *you* declare, so it pairs with an archetype
that declares often, and its value is the swap rather than the look — take
their only 6, or take away their only Aggression card to prevent a
counter-attack. The community rates it near the top of the deck but is clear
that the Prelude (discard the hand, redraw one more) is the stronger clause
and that the card is "balanced by raids": you are expected to use it once and
lose it ([BGG
3426912](https://boardgamegeek.com/thread/3426912/are-farseers-really-powerful-or-was-i-really-lucky)).
*Counterplay*: declare before they do — the lead player declares once per
round, so a rival's declaration denies Farseers its trigger that round — and
raid it. *Signature*: swaps per declaration (FINDINGS records 16 peeks and 14
swaps in 40 games, so the ability is live) and recycle usage, which moved from
0-of-998 to 2-of-102 once hand quality was priced.

**Sworn Guardians is the turtle's floor and its single point of failure.**
Covered under `turtle`: its own raid cost is 1 and it is the one card a raider
may still take. The forum agrees on the mechanism and rates the card only B.

**Lattice Spies is the cheapest tempo card in the deck.** Seizing normally
costs a whole card; Lattice Spies seizes in the Prelude for the card itself,
and because only one seize per round is permitted it denies the seize to
everyone else that round. It is an honourable mention on the community's
extra-turn list for exactly this reason. *Counterplay*: hold the initiative,
which makes the card dead, or seize first. *Signature*: seizes attributed to a
Prelude rather than to a burnt card.

**Secret Order removes the declaration tax on the two ambitions that pay it.**
*New this milestone; FINDINGS lists it as an open question — "a bot holding
either should declare far more freely, and none of them notices".* Declaring
Keeper or Empath means leading a 5 or a 6, the two best initiative cards in a
suit and the two lowest pip counts, and then zeroing it. Secret Order deletes
the zero marker on exactly those declarations, so the declarer keeps a 5 or 6
on the table and usually keeps the marker with it. The community rates the
card only B, which is a disagreement this catalog is happy to be tested on.
*Counterplay*: none direct — the card must be taken out of the row before it
is secured, or stolen after. *Signature*: Keeper/Empath declarations while
holding Secret Order versus without.

**Galactic Bards lets a follower declare, and changes what the leader dares to
do.** On a Surpass or Pivot, before actions, declare the ambition on your
played card if nobody has declared this round — and place no zero marker.
Combined with a Surpass it declares *and* takes the marker in one play, and it
is a Relic icon. The forum's observation is about its effect on other people:
with a Bards holder at the table, the initiative player is less willing to
*refrain* from declaring, since refraining gifts the declaration to the Bards
([BGG
3316877](https://boardgamegeek.com/thread/3316877/the-three-arcs-types-of-strategies-a-working-hypot)).
Its weakness is that its raid cost is 1, which is why the tier list puts it in
C against one dissent at "high B ~ low A" ([BGG
3229070](https://boardgamegeek.com/thread/3229070/arcs-guild-card-tier-list)).
*Counterplay*: declare first as the lead player, which closes the window for
the round; or raid it for one key. *Signature*: declarations by a
non-initiative player; the leader's declaration rate with and without a Bards
on the table.

**Gatekeepers turns the gate ring into a Warlord engine.** Gates have zero
building slots, so nothing is ever built there and raid dice may not be
collected unless the defender has no buildings anywhere — a gate battle
produces trophies and little else. Two extra dice is the largest single combat
modifier in the deck, and the six gates form a connected ring, so the fleet
that wins one is one move from the next. *New this milestone; the community
rates the card only B, and separately treats gates as defensive terrain to
clog rather than as a battlefield to farm, which is the opposite reading.*
*Counterplay*: fight on planets instead, where the defender's buildings both
absorb hits and enable raid dice.

**Song of Freedom removes a city for free.** "Return any one city you control"
means any city in a system you control, including a Rival's — no battle, no
Outrage, no cost — and it optionally seizes the initiative in the same breath;
the engine implements exactly this (`returnCityMaySeize`). It is also the only
route by which a city leaves the board *without* becoming a trophy, so unlike
a destroyed city it re-covers the owner's bonus slot immediately. Against
`greed`, whose Power comes from cities and the slots they uncover, that makes
it the single most efficient attack in the deck, and it is the counterplay
this catalog most wants the matrix to confirm. *New this milestone; not
discussed in any community source found.* *Counterplay*: keep fresh ships in
your city systems so no rival ever controls them — control is "more fresh
ships than each Rival", so parity is enough to deny it.

**Populist Demands is the sniper's declaration.** Declare any available
ambition on Securing it, from any seat, with no card number and no zero
marker. Covered under `sniper`; the catalog point is that there is exactly one
copy in the deck and it makes the line possible at all. *Counterplay*: take
it, or exhaust the markers before it is secured.

### Where the community contradicts the seeds

The disagreements are more useful than the agreements, because each is a cheap
experiment with two named sides. Collected here so K2 can pick them up
directly.

| claim | seed says | community says | how K2 settles it |
|---|---|---|---|
| surpass-bait | lead low to farm rivals' high cards | lead **high** to starve followers to one action ([3388220](https://boardgamegeek.com/thread/3388220/high-cards-starve-opponents-or-save-to-surpass)) | sweep mean lead-card rank; measure followers' pips per round |
| seizing | a tempo tool worth its card | contested: "force others to seize" vs "a card is one action, so seizing is cheap" ([3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs), [3436372](https://boardgamegeek.com/thread/3436372/pips-are-useless)) | sweep the `initiative` weight; the curve's peak is the answer |
| the quiet ambition | Keeper/Empath, because Relic and Psionic are scarce | Tyrant, because captives are scarce *and* cannot be raided back ([3408049](https://boardgamegeek.com/thread/3408049/its-a-hard-life-as-a-tyrant)) | two `turtle` presets, one per ambition pair |
| attacking the leader | `balancer` is a real archetype | mechanically unrewarded; attacking the *weak* player may be better ([3514660](https://boardgamegeek.com/thread/3514660)) | run `balancer` against its inverted twin |
| declaring | declare what you are winning | also declare what you cannot win, as decoy, fight-starter and denial ([3406482](https://boardgamegeek.com/thread/3406482/10-tips-or-reminders-for-arcs)) | count first places won on boxes the winner did not declare |
| resource pricing | `weapon` 0.5, below `material` 0.65 | Relics/Psionics, then Fuel/**Weapons**, then Material ([3223590](https://boardgamegeek.com/thread/3223590/a-low-key-guide-to-winning-base-arcs)) | one preset row swapping the two |
| Unions | not in the seed catalog at all | god-tier; three of four are S rank ([3229070](https://boardgamegeek.com/thread/3229070/arcs-guild-card-tier-list)) | the engine attaches one 0 times in 93 — fix the threshold, re-measure |
| `sniper` passivity | invest nothing until late | lie low *while building position*; the surge needs a card surplus, not idleness ([3344265](https://boardgamegeek.com/thread/3344265/when-do-you-like-to-declare-ambitions)) | the chapter-multiplier preset, with and without a raised `handCard` |
| Secret Order, Gatekeepers, Sworn Guardians | rated highly here on engine grounds | all three only B on the community tier list | per-card holding-versus-winning correlation over the K2 corpus |

Two community themes are recorded but deliberately **not** turned into
entries. Table talk and deal-making are described as first-class play in base
Arcs ([BGG
3652580](https://boardgamegeek.com/thread/3652580/what-does-good-arcs-play-look-like))
and this engine has no communication channel, so any archetype that depends on
negotiation is out of scope for the league. And the forum's central
meta-debate — whether Arcs is "a tactical game, not a strategy game", where
"your hand is where the power comes from, not the board" ([BGG
3522979](https://boardgamegeek.com/thread/3522979/any-decent-strategy-guides)),
against the case for genuine strategic layers ([BGG
3435205](https://boardgamegeek.com/thread/3435205/strategy-in-arcs-beyond-tactics))
— is exactly the question this whole plan is an instrument for. If the payoff
matrix comes back transitive and shallow, the tacticians are right about this
engine.

## Milestones

Prefixed K to stay clear of the main plan's M-series. Every agent produced
here respects the gauntlet budget (mean ≤ 30 ms/decision) — the league is
useless if it is too slow to play thousands of games.

### K1 — Catalog entries

Write the full schema entry for each seed hypothesis above. Mine BGG/discord
strategy discussion for lines the seeds miss. Deliverable: this file's catalog
section filled in. No code.

### K2 — Archetype presets and the payoff matrix

Implement each macro archetype as a **weight-preset on `greedy`** — a bias
vector over the existing eval terms plus at most a handful of new terms, not
new agent code. Run the full round-robin of archetype triples through the
paired runner (same protocol as the gauntlet: whole 6-seating blocks, printed
setup deck).

Deliverables: presets in `src/agents/`, a payoff-matrix tool in `tools/`, and
the matrix appended to the ledger below.

Pass rule per archetype: at least one **separated** positive result against
some other archetype. A preset that beats nothing is marked *folklore* in the
catalog and dropped from the league. The interesting output is
non-transitivity: cycles in the matrix are the empirical proof that 3p Arcs
has real strategic choices, and each cycle is a training axis the league must
cover.

### K3 — Signatures into the eval

Promote the signatures of surviving archetypes into `eval.ts` features and
**fit the weights from game outcomes** (logistic regression on final
placement, or CMA-ES directly on paired win-share). This is the first learned
exploitation of the catalog: human features, machine weights — the classic
recipe from Samuel's checkers onward, and it needs no network and no new
runtime cost.

The fitted agent is a gauntlet candidate under the standard
[GAUNTLET.md](GAUNTLET.md) protocol. Pass rule: the gauntlet's own.

### K4 — The league

Freeze the surviving archetypes as a permanent sparring set (same append-only
discipline as the anchors, listed in `src/agents/anchors.ts` alongside them).
The gauntlet gains per-archetype axes: a promotion candidate must not show a
separated regression against any league member — a bot that beats the newest
anchor but folds to `balancer` has learned an exploit, not strength.

### K5 — Learned artifacts (later, gated on K2–K4)

Only reached if the ladder stalls. In order of cost: behavior-cloning a policy
prior from the best mixture of league members (MCTS prior first, standalone
policy second); auxiliary heads predicting each ambition's winner; PPO
against opponents sampled from the league rather than pure self-play. Each is
a gauntlet candidate like any other; none of this changes the measurement
protocol.

An LLM can serve as the compiler at several points — generating detector
features and preset vectors from catalog prose (K2/K3), or judging state
pairs against the catalog to produce a shaped reward (K5, the Motif recipe) —
but the artifacts it emits go through the same pass rules as hand-written
ones.

## Ledger

Append-only. Payoff-matrix results from K2 land here; gauntlet promotions
stay in [GAUNTLET.md](GAUNTLET.md).

| date | matchup (candidate vs field) | games | diff ±95% | separated | verdict |
|---|---|---|---|---|---|

## What this plan does not claim

Injected knowledge is scaffolding, not ceiling. If K5 training runs long
enough, it will surpass and partly discard the catalog — that is the desired
outcome. The catalog's durable value is the league, the per-archetype
evaluation axes, and interpretability: the K3 signature features double as
probes for asking a trained net "are you turtling?". The same
catalog-schema → four-artifacts pipeline is game-agnostic and is the part
worth reusing beyond Arcs.

# Arcs — engine rules reference (base game)

Sources: **Arcs base rulebook, Leder Games, April 11 2024 edition** (24 pages),
plus the **official Arcs aid booklet** for the battle die faces. This document
is the ground truth the engine in `src/engine/` is coded against. Page numbers
in parentheses cite the rulebook unless marked "aid".

Component data that neither document contains is transcribed from the printed
game — the map, the 12 setup cards, the 31 Guild and Vox cards — except for two
values that are still reconstructed: the ambition markers' reverse sides and the
player-board economy. See [DATA-GAPS.md](DATA-GAPS.md) for both, and for how the
transcriptions were made and can be corrected.

Scope: **base game only**. Leaders & Lore (p21) and the Blighted Reach campaign
are out of scope; the engine has the hooks but ships no leader/lore data.

---

## 1. Victory and game length

A game lasts **up to 5 chapters** (p8). At the end of each chapter, the game
ends if any player has at least:

| Players | Power to end the game |
|---|---|
| 4 | 27 |
| 3 | 30 |
| 2 | 33 |

…or if the chapter marker is on chapter 5 (p19). The player with the most Power
wins; ties go to **the tied player earliest in turn order** (p19).

## 2. Components

- **Action cards (28)**: 4 suits × numbers 1–7. With 2–3 players the "1" and
  "7" cards are removed, leaving 20 cards numbered 2–6 (p3, p4 step C/D).
- **Court deck**: 25 Guild + 6 Vox cards, shuffled together (p4 step H).
- **Resource tokens (25)**: 5 each of Material, Fuel, Weapon, Relic, Psionic (p3).
- **Battle dice (18)**: 6 each of Assault, Skirmish, Raid (p3). All six faces of
  each die are printed in the aid booklet:

  | die | faces |
  |---|---|
  | Skirmish | 3× 1 hit, 3× blank |
  | Assault | 2 hits · 2 hits + self-hit · 1 hit + intercept · 1 hit + self-hit · 1 hit + self-hit · blank |
  | Raid | 2 keys + intercept · 1 key + self-hit · 1 building hit + 1 key · 1 building hit + self-hit · 1 building hit + self-hit · intercept |

  The intercept symbol is a ring that may enclose other symbols, so a face can
  intercept *and* deal its enclosed hits or keys.
- **Ambition markers (3)**: double-sided. Blue (starting) sides are
  **5/3**, **3/2**, **2/0** — first-place / second-place Power (p3).
- **Zero marker**, **initiative marker**, **chapter marker**.
- **Path markers (4)**, **out-of-play markers (6)**.
- Per player: **15 ships, 5 starports, 5 cities, 10 agents**, player board,
  Power marker (p2).

### 2.1 Action card pips

Pips are inversely related to the card number (p3, p8–p10):

| Card number | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| Action pips | 4 | 4 | 3 | 3 | 2 | 2 | 1 |

### 2.2 Suits and the actions their pips buy (p8)

| Suit | Actions |
|---|---|
| Administration | Tax, Repair, Influence |
| Aggression | Battle, Move, Secure |
| Construction | Build, Repair |
| Mobilization | Move, Influence |

### 2.3 Card number → ambition (p9)

| Card number | Ambition |
|---|---|
| 1 | none (4-player only) |
| 2 | Tycoon |
| 3 | Tyrant |
| 4 | Warlord |
| 5 | Keeper |
| 6 | Empath |
| 7 | any ambition (4-player only) |

## 3. The map (p6)

6 clusters, each with **4 systems: 1 gate + 3 planets**. The gate carries the
cluster number. The map centre (the Arcs logo) is not in play.

Adjacency:

- Each **gate** is adjacent to the 3 planets in its cluster and to the gates of
  its 2 neighbouring clusters (the gates form a ring).
- Each **planet** is adjacent to the gate in its cluster, and to one or both
  neighbouring planets. Planets separated by a thick, irregular border are not
  adjacent.

The rule says *neighbouring planets*, not *neighbouring planets in that
cluster* — the 18 planets are a ring, and a planet at the edge of a cluster
neighbours one in the next. Two of the six cluster boundaries carry a thin
border on the printed board and are therefore adjacent: **2.3 – 3.1** and
**5.3 – 6.1**. See [DATA-GAPS.md](DATA-GAPS.md#the-border-transcription).

Each planet has a **planet type** (one of the 5 resource types) and **1 or 2
building slots**.

**Out-of-play clusters.** In setup, 2 clusters (2–3 players) or 1 cluster
(4 players) are covered: a path marker over its gate and out-of-play markers
over its 3 planets (p4 step J). Those systems are out of play in every way. The
path marker joins the gates of the two neighbouring clusters, making them
adjacent for movement (p6).

## 4. Setup (p4–p5)

1. Give the initiative marker to a random player.
2. Shuffle the 20 "2"–"6" action cards; with 4 players shuffle in the eight
   "1" and "7" cards.
3. Ambition markers to the Available Markers area, blue side up.
4. Chapter marker on chapter 1; zero marker in the Ambition Declared slot.
5. Shuffle Vox + Guild into the Court deck. Deal a face-up Court row of
   **3 cards (2 players)** or **4 cards (3–4 players)**.
6. Draw 1 setup card for the player count; apply its out-of-play cluster.
7. **2 players only**: the 6 resource tokens matching the 6 covered planets go
   onto ambition boxes — Material and Fuel on Tycoon, Weapons on Warlord,
   Relics on Keeper, Psionics on Empath (p4 step K).
8. Each player, in turn order from the initiative holder, places:
   - 3 ships + 1 city in their **A** system,
   - 3 ships + 1 starport in their **B** system,
   - 2 ships in their **C** system (3–4 players) or 2 ships in each of two
     **C** systems (2 players).
9. Each player gains the 2 resource tokens matching their A and B planet types.
10. Each player draws 6 action cards. **2 players**: the player without
    initiative may mulligan their whole hand once (p19).
11. All action cards not in hands go face down to the action discard pile.

## 5. Playing a chapter (p8–p11)

A chapter is a series of rounds. In a round each player takes one turn,
starting with the initiative holder and proceeding clockwise.

### 5.1 Step 1 — the initiative holder leads

Play one action card **face up** as the lead card. You may take one action per
action pip on it, from its suit's action list.

**Passing initiative** (p8): if you have no cards, or you simply choose to skip,
give the initiative marker to the next clockwise player who has cards and
**immediately end the round**. That player leads the new round. If everyone
with cards passes consecutively, discard all action cards and end the chapter.

**Declaring an ambition** (p9) — before taking any actions, and only with the
initiative:

- Take the **highest-numbered available ambition marker** and place it in the
  declared ambition's box.
- Place the zero marker on the lead card, covering its number: the lead card's
  number is now **0**. Pip count is unchanged.
- You cannot declare if all 3 ambition markers are already placed.
- You cannot declare an ambition twice with the same card, but an ambition
  already declared in an earlier round may be declared again (another marker
  goes in its box).

### 5.2 Step 2 — every other player follows, clockwise

Players with no cards skip their turn. Otherwise play a card in one of three
ways:

| Option | Card played | Actions granted |
|---|---|---|
| **Surpass** | face up, **lead suit**, number **higher than the lead card** | pips on *your* card, your card's suit |
| **Copy** | any card, **face down** | exactly 1 action of the **lead** card |
| **Pivot** | face up, **not** the lead suit | exactly 1 action of **your** card |

Pivoting does not change the lead suit. Surpassing only needs to beat the lead
card, not every card played.

### 5.3 Seizing the initiative (p10)

When you play a card, before taking actions, you may seize the initiative
marker if you do not already hold it and nobody has seized it this round:

- play an **extra action card face down** beside your played card (you get no
  actions for it), **or**
- **Surpass with a "7"** card (4-player games only) — you still take its actions.

The initiative marker is laid down to show it has been seized; nobody else may
seize it this round.

### 5.4 Step 3 — check initiative (p11)

- If someone seized it, they keep it.
- Otherwise it goes to the player who Surpassed with the **highest card number**.
- If nobody Surpassed, it does not move.

### 5.5 Step 4 — discard and check for a new round (p11)

Discard all played cards, including seize cards, face down. If anyone still has
cards, start the next round; otherwise end the chapter.

## 6. Standard actions (p12–p16)

### Tax (Administration)
Choose a Loyal city, or a Rival city in a system **you control**. Gain 1
resource of that city's planet type from the supply. Taxing a Rival city also
**captures 1 agent** of that Rival from their supply, even if the resource
supply was empty. A given city cannot be taxed more than once per turn.

### Build (Construction)
Either:
- **Building**: place 1 starport or city in an empty building slot in a system
  containing a Loyal piece. Cities come from the leftmost city slot of your
  player board.
- **Ship**: place 1 ship at a Loyal starport. Each starport may build only one
  ship per turn.

Anything built in a system controlled by someone else is placed **damaged**.

### Move (Mobilization & Aggression)
Move any number of Loyal ships from one system to an adjacent system.

**Catapult**: if the ships start in a system with a Loyal starport, they may
keep moving — dropping ships off as desired — until they move into a gate
controlled by anyone else, or into any planet. Control is checked *before*
moving in, not after. You cannot Catapult from Rival starports you control.

You cannot move into out-of-play clusters; a path marker makes the two gates it
joins adjacent.

### Repair (Construction & Administration)
Stand up 1 damaged Loyal ship, or flip 1 damaged Loyal building. Anywhere on
the map.

### Influence (Mobilization & Administration)
Place 1 agent from your supply on any card in the Court.

### Secure (Aggression)
Take 1 card from the Court if you have **strictly more** Loyal agents on it than
each Rival. Return your agents on it to your supply and **capture** all Rival
agents from it. Resolve its *When Secured* text if any (if it says to discard
it, it goes to a face-up Court discard pile). Refill that Court slot from the
Court deck.

You cannot use Prelude actions on a card you secured in the same Prelude (p20).

### Battle (Aggression) — p14

1. **Choose battle system**: any system with Loyal ships. Those are the
   attacking ships.
2. **Choose defender**: one Rival with pieces there.
3. **Collect dice**: up to 1 die per attacking ship; any mix of assault,
   skirmish and raid, but at most 6 of any one type. **Raid dice may only be
   collected if there are defending buildings, or if the defender has no Loyal
   buildings anywhere on the map.**
4. **Roll and resolve, in this order**:
   1. Self-hits: hit any of your attacking ships once per self-hit symbol.
   2. Intercept: if any intercept symbol was rolled, hit your attacking ships
      once per **fresh defending ship**. This happens at most once per battle.
   3. Hits: hit defending ships once per hit symbol; once no defending ships
      remain, hits fall on defending buildings.
   4. Building hits: hit defending buildings once per building-hit symbol.
   5. Raid: spend keys to steal resources and Guild cards, **only if you still
      have attacking ships left**.

Hitting a fresh piece damages it; hitting a damaged piece destroys it. The
attacker takes destroyed defending pieces as **Trophies**; the defender takes
destroyed attacking pieces as Trophies. The attacker assigns all hits — the
defender makes no choices.

### Raiding (p16)
Spend keys to steal resources and Guild cards from the defender one at a time,
paying each item's **raid cost** (shown above its resource slot on the player
board, or in the Guild card's top-left corner).

### Destroying a city (p16) — even outside battle
1. **Provoke Outrage**: discard all resources and Guild cards you hold of that
   city's planet type, and place an agent on that type's Outrage icon on your
   player board. If that type is already Outraged, do everything except placing
   the agent.
2. **Ransack the Court**: Secure any one card that has any number of the
   defender's agents. All Rival agents on it become **Trophies**, not Captives.

Outraged resources cannot be spent for their Prelude action, but can still be
taxed and still count toward ambitions.

## 7. Resources (p17)

In your **Prelude** you may spend any number and mix of resource tokens:

| Resource | Prelude effect |
|---|---|
| Material | Take a Build or Repair action |
| Fuel | Take a Move action |
| Weapon | This turn, your card's action pips may also take Battle actions |
| Relic | Take a Secure action |
| Psionic | Take an action listed on the **lead** card |

Resources live in resource slots on the player board; building cities opens
more slots. When you gain a resource you may rearrange slots, but must discard
what you cannot hold. Each slot has a **raid cost**.

## 8. Prelude actions (p20)

Your Prelude begins when you play an action card and ends when you spend the
first action pip from it. Any number of Prelude actions may be taken, with two
limits: resources spent in the Prelude return to the supply only at the end of
the Prelude, and you cannot use Prelude actions on cards secured during that
same Prelude.

Declaring an ambition and seizing the initiative are decided **before** Prelude
actions. You cannot spend resources if you pass the initiative or have no cards.

## 9. Guild cards (p17)

Three parts: **suit** (one of the 5 resource types), **rules text**, and **raid
cost**. The suit counts toward ambitions exactly like a resource token —
Material and Fuel cards to Tycoon, Relic cards to Keeper, Psionic cards to
Empath. **Weapon cards count toward no ambition.**

## 10. Ending a chapter (p18–p19)

The chapter ends when everyone is out of action cards, or when everyone holding
cards passes the initiative consecutively.

### Step 1 — score ambitions

Score every ambition box holding at least one marker:

| Ambition | Counts |
|---|---|
| Tycoon | most total Fuel + Material icons (resources and Guild cards) |
| Tyrant | most Captives |
| Warlord | most Trophies |
| Keeper | most Relic icons |
| Empath | most Psionic icons |

First place gains the **higher** Power summed over all markers in that box;
second place gains the **lower** Power summed over all markers.

- **Ties for first**: all tied players score **second place** instead.
- **Ties for second**: nobody places, no Power.
- **Qualifying**: you cannot score an ambition if you have none of the thing
  counted.
- **Bonus city Power**: each time you take first place **outright** (not tied),
  gain +2 Power if your "+2 to won ambitions" city slot is uncovered, or +5 if
  both the "+2" and "+3" slots are uncovered. Once per ambition regardless of
  marker count.

**2-player scoring** (p19): the 6 resources placed on ambition boxes during
setup count as if a third player held them; Weapons on the Warlord box count as
Trophies.

### Step 2 — clean up and flip

If Warlord scored, return all Trophies; if Tyrant scored, return all Captives.
Return all ambition markers to the Available area. Then **flip the
lowest-Power ambition marker that has not yet been flipped** to its
higher-Power side.

### Step 3 — end game or advance chapter

End if any player is at the Power threshold, or the chapter marker is on
chapter 5. Otherwise advance the chapter marker.

### Step 4 — draw cards

Shuffle all action cards; every player draws 6. Cards not in hands go to the
discard pile. 2 players: the player without initiative may mulligan once.

## 11. Engine rulings

Places where the rulebook leaves a choice the engine resolves itself rather
than surfacing as its own decision node. Each is a deliberate, documented
simplification, not a rules reading.

There are ten, in two groups: five in the core rules, and five that only come up
once every Court card ability is live.

- **Ransacking the Court** (p16) says "Secure *any* card that has any number of
  the defender's agents". The engine picks the card holding the most of the
  defender's agents, breaking ties leftmost.
- **Paying for an action** when both a card pip and a Prelude resource could
  cover it: the engine spends the resource grant first, most-restrictive grant
  first. Pips are more flexible, so holding them back is almost always right.
- **Catapult moves** may not re-enter a system the same catapult has already
  passed through. The rulebook sets no such limit, but without one a catapult
  can shuttle between two uncontrolled gates forever; revisiting can only undo
  progress, so nothing legal is lost.
- **Hits are assigned one at a time.** The rulebook lets the attacker place a
  whole volley at once; the engine asks per hit, which is the same set of
  outcomes with an enumerable action space.
- **Resource slot rearranging** (p17) is automatic: gained resources fill the
  leftmost empty open slot, and a returning city discards from the right.

### Court card rulings

Five more come from card interactions the cards themselves do not settle.

- **A Cartel's stockpile when the card leaves play.** The card says it keeps its
  resource type's supply, but not what happens when it is discarded. The engine
  returns the stockpile to the general supply, which is the only reading that
  keeps the tokens in the game. Tokens returned *while* a Cartel is in play flow
  onto the card, and Rivals' post-scoring discards go there too.
- **Only blank skirmish dice may be rerolled** (Skirmishers). A skirmish face is
  either one hit or nothing, and the die never hurts the attacker, so rerolling a
  face that already hit is strictly dominated. Offering only blanks removes no
  reachable outcome and keeps the branching small.
- **Execute takes Captives in capture order** (Prison Wardens). Which Rival an
  executed agent belonged to only matters when Warlord scores and Trophies go
  home, and the card gives the player no say in the choice.
- **"Shuffle into the Court deck" puts the card on the bottom** (Song of Freedom,
  Guild Struggle). Securing is a decision node with no RNG in scope by design, so
  the engine uses a defined order instead. The Court deck's order is hidden
  information either way, and `determinize()` reshuffles it for search.
- **Sworn Guardians does not block Elder Broker's Trade.** Its text is "Rivals
  cannot steal your resources", and the rulebook uses *steal* as a keyword —
  raids steal, Silver-Tongues steals. Trade says "swap" and hands something back,
  so it is not a theft.

## 12. Fine print used by the engine (p22)

- **Control**: you control a system and its contents if you have **more fresh
  ships** there than each Rival. Ties mean nobody controls it.
- **Fresh / damaged / destroyed**: pieces start fresh; a hit damages a fresh
  piece and destroys a damaged one.
- **Unspecified ties and unclear decision order** resolve in turn order,
  starting from the initiative holder and going clockwise.
- **Elimination**: a player with no starports or ships on the map places 3 fresh
  ships in any gate at the end of their turn.
- **Piece limits** are the contents of the box. If you must place more pieces
  than possible, place the maximum possible.
- Cities return to the **rightmost empty city slot** of their original player
  board; other pieces return to their supply.
- Players may not show their hand to anyone.

# Train Ticket Booking System

A simple Java-based train ticket booking system for testing DV8 software architecture analysis.

## Architecture

**Type:** Monolithic application
**Pattern:** 3-tier layered architecture (similar to Library Management System)
**Total Classes:** 11

### Class Structure

```
TTS/
├── Person.java              (Base class - inheritance hierarchy)
├── Passenger.java           (Customer entity)
├── Staff.java               (Staff base class)
├── TicketAgent.java         (Staff role - books/cancels tickets)
├── StationManager.java      (Staff role - manages schedules)
├── Train.java               (Train entity)
├── Route.java               (Route entity)
├── TrainStation.java        (Station entity)
├── Ticket.java              (Booking transaction)
├── TicketBookingSystem.java (Service façade - Singleton)
└── Main.java                (Entry point - demo)
```

### Dependencies

**Entity Layer:**
- `Person` → base for `Passenger`, `Staff`
- `Staff` → base for `TicketAgent`, `StationManager`
- `Train`, `Route`, `TrainStation`, `Ticket` → independent entities

**Service Layer:**
- `TicketBookingSystem` → manages all entities (central façade)
- `TicketAgent` → uses `Ticket`, `Passenger`, `Route`, `Train`
- `StationManager` → uses `Train`, `TrainStation`

**Presentation Layer:**
- `Main` → uses `TicketBookingSystem` and all entities

### Expected DV8 Metrics

**M-Score:** ~0.6-0.7 (moderate modularity)
- Good: Clear inheritance hierarchy
- Good: Singleton service pattern
- Issue: Some coupling through TicketBookingSystem façade

**Propagation Cost:** ~30-40%
- Moderate coupling through central service

**Core Size:** ~20-30%
- TicketBookingSystem and Person hierarchy in core

## Compilation

```bash
cd /Users/chrisharing/Desktop/RA Software Architecture Analsysis/AGENT/TEST_AUTO/SMALL_PROJECTS/TrainTicketSystem
javac -d bin src/TTS/*.java
```

## Running

```bash
java -cp bin TTS.Main
```

## Features

- **Passenger Management:** Register passengers, track bookings
- **Train Management:** Add trains, check availability
- **Route Management:** Define routes with intermediate stops
- **Ticket Booking:** Book/cancel tickets with seat allocation
- **Staff Operations:** Ticket agents and station managers
- **Station Search:** Find trains between stations

## DV8 Analysis

To analyze with DV8:

```bash
cd /Users/chrisharing/Desktop/RA Software Architecture Analsysis/AGENT/TEST_AUTO/01_STAGE_ANALYZE

python3 dv8_agent.py \
  --repo ../SMALL_PROJECTS/TrainTicketSystem \
  --ask all
```

## Hand-Calculated Dependencies

Total files: 11
Expected dependencies: ~25-30

**Inheritance (5):**
- Person → Passenger
- Person → Staff
- Staff → TicketAgent
- Staff → StationManager

**Composition/Association (~20-25):**
- TicketBookingSystem → all entities
- TicketAgent → Ticket, Passenger, Train, Route
- Ticket → Passenger, Train, Route
- Train → Route
- Route → TrainStation
- TrainStation → Train, TicketAgent
- etc.

## Comparison to Library Management System

| Metric | Library System | Train System |
|--------|---------------|--------------|
| Classes | 11 | 11 |
| Lines of Code | ~2600 | ~800 |
| Inheritance Levels | 2 | 2 |
| Pattern | Singleton service | Singleton service |
| Type | Monolith | Monolith |

## Future Enhancements

For temporal/issue tracking testing:
1. Add database layer (introduce coupling)
2. Create "god class" version (poor architecture)
3. Add git history with bugs/features
4. Refactor into microservices version

## License

Created for educational/testing purposes.

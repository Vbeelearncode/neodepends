package TTS;

/**
 * Main entry point for Train Ticket Booking System
 * Demonstrates the system functionality
 */
public class Main {
    public static void main(String[] args) {
        System.out.println("╔══════════════════════════════════════════════╗");
        System.out.println("║   TRAIN TICKET BOOKING SYSTEM - DEMO        ║");
        System.out.println("╚══════════════════════════════════════════════╝\n");

        // Get system instance
        TicketBookingSystem system = TicketBookingSystem.getInstance();

        // Create stations
        TrainStation newyork = new TrainStation("NYC", "New York Penn Station", "New York");
        TrainStation boston = new TrainStation("BOS", "Boston South Station", "Boston");
        TrainStation philly = new TrainStation("PHL", "Philadelphia 30th Street", "Philadelphia");

        system.addStation(newyork);
        system.addStation(boston);
        system.addStation(philly);

        // Create routes
        Route route1 = new Route("R001", newyork, boston, 215.0);
        route1.addIntermediateStop(philly);
        Route route2 = new Route("R002", newyork, philly, 95.0);

        system.addRoute(route1);
        system.addRoute(route2);

        // Create trains
        Train acela = new Train("TR001", "Acela Express", 300, "Express");
        acela.setRoute(route1);
        Train northeast = new Train("TR002", "Northeast Regional", 400, "Regional");
        northeast.setRoute(route2);

        system.addTrain(acela);
        system.addTrain(northeast);

        newyork.addTrain(acela);
        newyork.addTrain(northeast);

        // Create staff
        TicketAgent agent1 = new TicketAgent("John Smith", "EMP001",
                "john@railway.com", "555-0100", "AGT001", 45000.0);
        agent1.setAssignedStation(newyork);

        StationManager manager1 = new StationManager("Mary Johnson", "EMP002",
                "mary@railway.com", "555-0200", "MGR001", 75000.0);
        manager1.setManagedStation(newyork);
        manager1.addTrainSchedule(acela);
        manager1.addTrainSchedule(northeast);

        newyork.addAgent(agent1);
        system.addStaff(agent1);
        system.addStaff(manager1);

        // Create passengers
        Passenger passenger1 = new Passenger("Alice Brown", "P001",
                "alice@email.com", "555-1000", "P123456");
        Passenger passenger2 = new Passenger("Bob Wilson", "P002",
                "bob@email.com", "555-2000", "P789012");

        system.registerPassenger(passenger1);
        system.registerPassenger(passenger2);

        // Display initial system state
        system.displaySystemStats();

        System.out.println("\n═══════ DEMONSTRATION ═══════\n");

        // Demo: Search for trains
        System.out.println("1. Searching for trains from NYC to Boston...");
        var availableTrains = system.searchAvailableTrains(newyork, boston);
        for (Train t : availableTrains) {
            t.displayInfo();
            System.out.println();
        }

        // Demo: Book tickets
        System.out.println("\n2. Booking tickets for Alice...");
        Ticket ticket1 = agent1.issueTicket(passenger1, route1, acela, "A1", route1.getBaseFare());
        ticket1.displayTicket();

        System.out.println("\n3. Booking tickets for Bob...");
        Ticket ticket2 = agent1.issueTicket(passenger2, route2, northeast, "B5", route2.getBaseFare());
        ticket2.displayTicket();

        // Display passenger info
        System.out.println("\n4. Passenger Information:");
        passenger1.displayInfo();
        System.out.println();
        passenger2.displayInfo();

        // Display staff performance
        System.out.println("\n5. Staff Performance:");
        agent1.displayInfo();
        System.out.println();
        manager1.displayInfo();

        // Demo: Cancel a ticket
        System.out.println("\n6. Cancelling Bob's ticket...");
        agent1.cancelTicket(passenger2, ticket2);
        ticket2.displayTicket();

        // Final system stats
        System.out.println("\n7. Final System State:");
        system.displaySystemStats();
        acela.displayInfo();

        System.out.println("\n╔══════════════════════════════════════════════╗");
        System.out.println("║          DEMO COMPLETE                       ║");
        System.out.println("╚══════════════════════════════════════════════╝");
    }
}

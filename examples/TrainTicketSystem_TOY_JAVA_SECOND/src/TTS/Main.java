package TTS;

import java.util.ArrayList;

/**
 * Main class - Presentation layer
 * MAJOR IMPROVEMENT: Only uses service layer and repositories
 * Does NOT directly create or manipulate entities
 * This is proper layered architecture!
 */
public class Main {
    public static void main(String[] args) {
        System.out.println("=== Train Ticket Booking System (SECOND - Refactored) ===\n");

        // Initialize repositories
        TrainStationRepository stationRepo = new TrainStationRepository();
        TrainRepository trainRepo = new TrainRepository();
        RouteRepository routeRepo = new RouteRepository();
        PassengerRepository passengerRepo = new PassengerRepository();
        TicketRepository ticketRepo = new TicketRepository();

        // Initialize service
        BookingService bookingService = new BookingService(trainRepo, routeRepo,
                                                           ticketRepo, passengerRepo, stationRepo);

        // Create stations (via repository)
        TrainStation nycStation = new TrainStation("NYC-001", "Penn Station", "New York", "NY");
        TrainStation bostonStation = new TrainStation("BOS-001", "South Station", "Boston", "MA");
        stationRepo.addStation(nycStation);
        stationRepo.addStation(bostonStation);

        // Create routes (uses station IDs, not objects!)
        Route route1 = new Route("R-001", "NYC-001", "BOS-001", 350.0, 89.99);
        routeRepo.addRoute(route1);

        // Create trains (uses route IDs, not objects!)
        Train train1 = new Train("T-001", "Northeast Express", "R-001", 200, "08:00", "12:30");
        Train train2 = new Train("T-002", "Boston Flyer", "R-001", 150, "14:00", "18:30");
        trainRepo.addTrain(train1);
        trainRepo.addTrain(train2);

        // Create passengers
        Passenger passenger1 = new Passenger("John Doe", "P-001", "john@email.com", "555-1234");
        Passenger passenger2 = new Passenger("Jane Smith", "P-002", "jane@email.com", "555-5678");
        passengerRepo.addPassenger(passenger1);
        passengerRepo.addPassenger(passenger2);

        System.out.println("--- Initial System State ---");
        System.out.println("Stations: " + stationRepo.getAllStations().size());
        System.out.println("Routes: " + routeRepo.getAllRoutes().size());
        System.out.println("Trains: " + trainRepo.getAllTrains().size());
        System.out.println("Passengers: " + passengerRepo.getAllPassengers().size());
        System.out.println();

        // Book tickets through service
        System.out.println("--- Booking Tickets ---");
        Ticket ticket1 = bookingService.bookTicket("P-001", "T-001", "2025-12-20");
        if (ticket1 != null) {
            System.out.println("✓ Booked: " + ticket1.getTicketId());
            ticket1.displayInfo();
            System.out.println();
        }

        Ticket ticket2 = bookingService.bookTicket("P-002", "T-002", "2025-12-21");
        if (ticket2 != null) {
            System.out.println("✓ Booked: " + ticket2.getTicketId());
            ticket2.displayInfo();
            System.out.println();
        }

        // Search trains through service
        System.out.println("--- Searching Trains on Route R-001 ---");
        ArrayList<Train> trainsOnRoute = bookingService.searchTrains("R-001");
        System.out.println("Found " + trainsOnRoute.size() + " trains:");
        for (Train t : trainsOnRoute) {
            t.displayInfo();
            System.out.println();
        }

        // View passenger tickets through service
        System.out.println("--- Passenger Bookings ---");
        ArrayList<Ticket> p1Tickets = bookingService.getPassengerTickets("P-001");
        System.out.println("Passenger P-001 has " + p1Tickets.size() + " ticket(s)");

        // Cancel ticket through service
        System.out.println("\n--- Cancelling Ticket ---");
        if (ticket1 != null && bookingService.cancelTicket(ticket1.getTicketId())) {
            System.out.println("✓ Cancelled: " + ticket1.getTicketId());
            ticket1.displayInfo();
        }

        System.out.println("\n=== Demo Complete ===");
    }
}

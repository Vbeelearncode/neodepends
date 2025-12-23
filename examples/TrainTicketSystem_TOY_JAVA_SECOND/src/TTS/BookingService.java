package TTS;

import java.util.ArrayList;

/**
 * NEW CLASS: Service layer for booking operations
 * REPLACES the god class from FIRST version
 * Uses repositories instead of managing entities directly
 */
public class BookingService {
    private TrainRepository trainRepo;
    private RouteRepository routeRepo;
    private TicketRepository ticketRepo;
    private PassengerRepository passengerRepo;
    private TrainStationRepository stationRepo;

    public BookingService(TrainRepository trainRepo, RouteRepository routeRepo,
                          TicketRepository ticketRepo, PassengerRepository passengerRepo,
                          TrainStationRepository stationRepo) {
        this.trainRepo = trainRepo;
        this.routeRepo = routeRepo;
        this.ticketRepo = ticketRepo;
        this.passengerRepo = passengerRepo;
        this.stationRepo = stationRepo;
    }

    public Ticket bookTicket(String passengerId, String trainId, String travelDate) {
        Passenger passenger = passengerRepo.getPassenger(passengerId);
        Train train = trainRepo.getTrain(trainId);

        if (passenger == null || train == null) {
            return null;
        }

        if (!train.bookSeat()) {
            System.out.println("No seats available");
            return null;
        }

        String ticketId = "TKT-" + System.currentTimeMillis();
        String seatNumber = String.valueOf(train.getTotalSeats() - train.getAvailableSeats());
        Route route = routeRepo.getRoute(train.getRouteId());
        double fare = route != null ? route.getBaseFare() : 0.0;

        Ticket ticket = new Ticket(ticketId, passengerId, train.getRouteId(),
                                   trainId, seatNumber, fare, travelDate);

        ticketRepo.addTicket(ticket);
        passenger.addTicket(ticketId);

        return ticket;
    }

    public boolean cancelTicket(String ticketId) {
        Ticket ticket = ticketRepo.getTicket(ticketId);
        if (ticket == null || ticket.getStatus().equals("CANCELLED")) {
            return false;
        }

        Train train = trainRepo.getTrain(ticket.getTrainId());
        if (train != null) {
            train.cancelSeat();
        }

        ticket.cancel();
        return true;
    }

    public ArrayList<Train> searchTrains(String routeId) {
        return trainRepo.getTrainsByRoute(routeId);
    }

    public ArrayList<Ticket> getPassengerTickets(String passengerId) {
        return ticketRepo.getTicketsByPassenger(passengerId);
    }
}

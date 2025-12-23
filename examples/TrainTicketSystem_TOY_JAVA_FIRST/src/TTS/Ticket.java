package TTS;

import java.time.LocalDateTime;
import java.time.format.DateTimeFormatter;

/**
 * Represents a train ticket
 */
public class Ticket {
    private static int ticketCounter = 1000;

    private String ticketId;
    private Passenger passenger;
    private Route route;
    private Train train;
    private String seatNumber;
    private double price;
    private LocalDateTime bookingTime;
    private String status; // BOOKED, CANCELLED, COMPLETED

    public Ticket(Passenger passenger, Route route, Train train,
                  String seatNumber, double price) {
        this.ticketId = "TKT" + (ticketCounter++);
        this.passenger = passenger;
        this.route = route;
        this.train = train;
        this.seatNumber = seatNumber;
        this.price = price;
        this.bookingTime = LocalDateTime.now();
        this.status = "BOOKED";
    }

    public void cancel() {
        this.status = "CANCELLED";
        train.releaseSeat(seatNumber);
    }

    public void complete() {
        this.status = "COMPLETED";
    }

    public String getTicketId() {
        return ticketId;
    }

    public Passenger getPassenger() {
        return passenger;
    }

    public String getSeatNumber() {
        return seatNumber;
    }

    public double getPrice() {
        return price;
    }

    public String getStatus() {
        return status;
    }

    public void displayTicket() {
        DateTimeFormatter formatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm");
        System.out.println("╔══════════════ TRAIN TICKET ══════════════╗");
        System.out.println("  Ticket ID: " + ticketId);
        System.out.println("  Passenger: " + passenger.getName());
        System.out.println("  Train: " + train.getTrainName() + " (" + train.getTrainNumber() + ")");
        System.out.println("  Route: " + route.getOrigin().getName() + " → " + route.getDestination().getName());
        System.out.println("  Seat: " + seatNumber);
        System.out.println("  Price: $" + String.format("%.2f", price));
        System.out.println("  Booked: " + bookingTime.format(formatter));
        System.out.println("  Status: " + status);
        System.out.println("╚══════════════════════════════════════════╝");
    }
}

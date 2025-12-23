package TTS;

import java.time.LocalDateTime;
import java.time.format.DateTimeFormatter;

/**
 * Ticket entity
 * IMPROVED: Uses IDs instead of object references
 */
public class Ticket {
    private String ticketId;
    private String passengerId;  // CHANGED: Was Passenger object
    private String routeId;      // CHANGED: Was Route object
    private String trainId;      // CHANGED: Was Train object
    private String seatNumber;
    private double fare;
    private String bookingDate;
    private String travelDate;
    private String status;

    public Ticket(String ticketId, String passengerId, String routeId, String trainId,
                  String seatNumber, double fare, String travelDate) {
        this.ticketId = ticketId;
        this.passengerId = passengerId;
        this.routeId = routeId;
        this.trainId = trainId;
        this.seatNumber = seatNumber;
        this.fare = fare;
        this.travelDate = travelDate;
        this.bookingDate = LocalDateTime.now().format(DateTimeFormatter.ISO_LOCAL_DATE_TIME);
        this.status = "CONFIRMED";
    }

    public void cancel() {
        this.status = "CANCELLED";
    }

    public String getTicketId() { return ticketId; }
    public String getPassengerId() { return passengerId; }
    public String getRouteId() { return routeId; }
    public String getTrainId() { return trainId; }
    public String getSeatNumber() { return seatNumber; }
    public double getFare() { return fare; }
    public String getBookingDate() { return bookingDate; }
    public String getTravelDate() { return travelDate; }
    public String getStatus() { return status; }

    public void displayInfo() {
        System.out.println("Ticket: " + ticketId + " [" + status + "]");
        System.out.println("Passenger: " + passengerId);
        System.out.println("Train: " + trainId + " on Route: " + routeId);
        System.out.println("Seat: " + seatNumber + ", Fare: $" + fare);
        System.out.println("Travel Date: " + travelDate);
    }
}

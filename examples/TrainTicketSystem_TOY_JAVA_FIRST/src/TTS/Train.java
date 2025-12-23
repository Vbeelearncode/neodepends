package TTS;

import java.util.ArrayList;

/**
 * Represents a train with routes and seats
 */
public class Train {
    private String trainNumber;
    private String trainName;
    private int totalSeats;
    private ArrayList<String> bookedSeats;
    private Route route;
    private String type; // Express, Local, etc.

    public Train(String trainNumber, String trainName, int totalSeats, String type) {
        this.trainNumber = trainNumber;
        this.trainName = trainName;
        this.totalSeats = totalSeats;
        this.type = type;
        this.bookedSeats = new ArrayList<>();
    }

    public void setRoute(Route route) {
        this.route = route;
    }

    public Route getRoute() {
        return route;
    }

    public String getTrainNumber() {
        return trainNumber;
    }

    public String getTrainName() {
        return trainName;
    }

    public int getTotalSeats() {
        return totalSeats;
    }

    public int getAvailableSeats() {
        return totalSeats - bookedSeats.size();
    }

    public boolean isSeatAvailable(String seatNumber) {
        return !bookedSeats.contains(seatNumber);
    }

    public boolean bookSeat(String seatNumber) {
        if (isSeatAvailable(seatNumber)) {
            bookedSeats.add(seatNumber);
            return true;
        }
        return false;
    }

    public void releaseSeat(String seatNumber) {
        bookedSeats.remove(seatNumber);
    }

    public void displayInfo() {
        System.out.println("Train: " + trainName + " (" + trainNumber + ")");
        System.out.println("Type: " + type);
        System.out.println("Available Seats: " + getAvailableSeats() + "/" + totalSeats);
        if (route != null) {
            System.out.println("Route: " + route.getOrigin() + " → " + route.getDestination());
        }
    }
}

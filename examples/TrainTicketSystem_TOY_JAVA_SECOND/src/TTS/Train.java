package TTS;

/**
 * Train entity
 * MAJOR IMPROVEMENT: Uses route ID instead of Route object
 * This breaks the cyclic dependency!
 */
public class Train {
    private String trainId;
    private String trainName;
    private String routeId;  // CHANGED: Was Route object
    private int totalSeats;
    private int availableSeats;
    private String departureTime;
    private String arrivalTime;

    public Train(String trainId, String trainName, String routeId, int totalSeats, String departureTime, String arrivalTime) {
        this.trainId = trainId;
        this.trainName = trainName;
        this.routeId = routeId;
        this.totalSeats = totalSeats;
        this.availableSeats = totalSeats;
        this.departureTime = departureTime;
        this.arrivalTime = arrivalTime;
    }

    public boolean bookSeat() {
        if (availableSeats > 0) {
            availableSeats--;
            return true;
        }
        return false;
    }

    public void cancelSeat() {
        if (availableSeats < totalSeats) {
            availableSeats++;
        }
    }

    public String getTrainId() { return trainId; }
    public String getTrainName() { return trainName; }
    public String getRouteId() { return routeId; }
    public int getTotalSeats() { return totalSeats; }
    public int getAvailableSeats() { return availableSeats; }
    public String getDepartureTime() { return departureTime; }
    public String getArrivalTime() { return arrivalTime; }

    public void displayInfo() {
        System.out.println("Train: " + trainName + " (" + trainId + ")");
        System.out.println("Route: " + routeId);
        System.out.println("Schedule: " + departureTime + " - " + arrivalTime);
        System.out.println("Seats: " + availableSeats + "/" + totalSeats);
    }
}

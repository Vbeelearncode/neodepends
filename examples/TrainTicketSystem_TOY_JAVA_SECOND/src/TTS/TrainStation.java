package TTS;

/**
 * TrainStation entity
 * MAJOR IMPROVEMENT: Removed all collections and bidirectional dependencies
 * Now a pure entity with only its own data
 */
public class TrainStation {
    private String stationId;
    private String stationName;
    private String city;
    private String state;

    // REMOVED: ArrayList<TicketAgent> agents
    // REMOVED: ArrayList<Train> availableTrains
    // These are now managed by repositories!

    public TrainStation(String stationId, String stationName, String city, String state) {
        this.stationId = stationId;
        this.stationName = stationName;
        this.city = city;
        this.state = state;
    }

    public String getStationId() { return stationId; }
    public String getStationName() { return stationName; }
    public String getCity() { return city; }
    public String getState() { return state; }

    public void displayInfo() {
        System.out.println("Station: " + stationName + " (" + stationId + ")");
        System.out.println("Location: " + city + ", " + state);
    }
}

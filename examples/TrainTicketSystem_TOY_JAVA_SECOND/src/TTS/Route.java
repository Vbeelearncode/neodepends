package TTS;

import java.util.ArrayList;

/**
 * Route entity
 * MAJOR IMPROVEMENT: Uses station IDs instead of TrainStation objects
 * This breaks the cyclic dependency!
 */
public class Route {
    private String routeId;
    private String originStationId;        // CHANGED: Was TrainStation object
    private String destinationStationId;   // CHANGED: Was TrainStation object
    private ArrayList<String> intermediateStopIds;  // CHANGED: Was ArrayList<TrainStation>
    private double distance;
    private double baseFare;

    public Route(String routeId, String originStationId, String destinationStationId, double distance, double baseFare) {
        this.routeId = routeId;
        this.originStationId = originStationId;
        this.destinationStationId = destinationStationId;
        this.intermediateStopIds = new ArrayList<>();
        this.distance = distance;
        this.baseFare = baseFare;
    }

    public void addIntermediateStop(String stationId) {
        intermediateStopIds.add(stationId);
    }

    public String getRouteId() { return routeId; }
    public String getOriginStationId() { return originStationId; }
    public String getDestinationStationId() { return destinationStationId; }
    public ArrayList<String> getIntermediateStopIds() { return intermediateStopIds; }
    public double getDistance() { return distance; }
    public double getBaseFare() { return baseFare; }

    public void displayInfo() {
        System.out.println("Route: " + routeId);
        System.out.println("From: " + originStationId + " To: " + destinationStationId);
        System.out.println("Distance: " + distance + " km, Fare: $" + baseFare);
        System.out.println("Stops: " + intermediateStopIds.size());
    }
}

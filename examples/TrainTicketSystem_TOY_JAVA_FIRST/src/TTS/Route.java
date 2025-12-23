package TTS;

import java.util.ArrayList;

/**
 * Represents a route between stations
 */
public class Route {
    private String routeId;
    private TrainStation origin;
    private TrainStation destination;
    private ArrayList<TrainStation> intermediateStops;
    private double distance; // in km
    private double baseFare;

    public Route(String routeId, TrainStation origin, TrainStation destination, double distance) {
        this.routeId = routeId;
        this.origin = origin;
        this.destination = destination;
        this.distance = distance;
        this.intermediateStops = new ArrayList<>();
        this.baseFare = calculateFare();
    }

    public void addIntermediateStop(TrainStation station) {
        intermediateStops.add(station);
    }

    private double calculateFare() {
        // Simple fare calculation: $0.10 per km
        return distance * 0.10;
    }

    public String getRouteId() {
        return routeId;
    }

    public TrainStation getOrigin() {
        return origin;
    }

    public TrainStation getDestination() {
        return destination;
    }

    public double getDistance() {
        return distance;
    }

    public double getBaseFare() {
        return baseFare;
    }

    public ArrayList<TrainStation> getIntermediateStops() {
        return intermediateStops;
    }

    public void displayInfo() {
        System.out.println("Route ID: " + routeId);
        System.out.println("Origin: " + origin.getName());
        System.out.println("Destination: " + destination.getName());
        System.out.println("Distance: " + distance + " km");
        System.out.println("Base Fare: $" + baseFare);
        if (!intermediateStops.isEmpty()) {
            System.out.print("Stops: ");
            for (TrainStation stop : intermediateStops) {
                System.out.print(stop.getName() + " ");
            }
            System.out.println();
        }
    }
}

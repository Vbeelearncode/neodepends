package TTS;

import java.util.ArrayList;
import java.util.HashMap;

/**
 * NEW CLASS: Repository pattern
 * Manages Route entities separately
 */
public class RouteRepository {
    private HashMap<String, Route> routes;

    public RouteRepository() {
        this.routes = new HashMap<>();
    }

    public void addRoute(Route route) {
        routes.put(route.getRouteId(), route);
    }

    public Route getRoute(String routeId) {
        return routes.get(routeId);
    }

    public ArrayList<Route> getAllRoutes() {
        return new ArrayList<>(routes.values());
    }

    public ArrayList<Route> getRoutesByStation(String stationId) {
        ArrayList<Route> result = new ArrayList<>();
        for (Route route : routes.values()) {
            if (route.getOriginStationId().equals(stationId) ||
                route.getDestinationStationId().equals(stationId) ||
                route.getIntermediateStopIds().contains(stationId)) {
                result.add(route);
            }
        }
        return result;
    }
}

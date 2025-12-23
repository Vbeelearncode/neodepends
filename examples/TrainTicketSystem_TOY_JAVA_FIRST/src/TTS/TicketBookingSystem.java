package TTS;

import java.util.ArrayList;

/**
 * Main service class - Singleton pattern (like Library.java in LMS)
 * Coordinates all booking operations
 * Extends BaseManagementSystem for logging and statistics
 */
public class TicketBookingSystem extends BaseManagementSystem {
    private static TicketBookingSystem instance;

    private ArrayList<TrainStation> stations;
    private ArrayList<Train> trains;
    private ArrayList<Route> routes;
    private ArrayList<Passenger> passengers;
    private ArrayList<Staff> staff;

    private TicketBookingSystem() {
        this.stations = new ArrayList<>();
        this.trains = new ArrayList<>();
        this.routes = new ArrayList<>();
        this.passengers = new ArrayList<>();
        this.staff = new ArrayList<>();
    }

    public static TicketBookingSystem getInstance() {
        if (instance == null) {
            instance = new TicketBookingSystem();
        }
        return instance;
    }

    // Station management
    public void addStation(TrainStation station) {
        stations.add(station);
    }

    public TrainStation findStation(String stationCode) {
        for (TrainStation station : stations) {
            if (station.getStationCode().equals(stationCode)) {
                return station;
            }
        }
        return null;
    }

    // Train management
    public void addTrain(Train train) {
        trains.add(train);
    }

    public Train findTrain(String trainNumber) {
        for (Train train : trains) {
            if (train.getTrainNumber().equals(trainNumber)) {
                return train;
            }
        }
        return null;
    }

    // Route management
    public void addRoute(Route route) {
        routes.add(route);
    }

    public ArrayList<Route> findRoutes(TrainStation origin, TrainStation destination) {
        ArrayList<Route> matchingRoutes = new ArrayList<>();
        for (Route route : routes) {
            if (route.getOrigin().equals(origin) &&
                route.getDestination().equals(destination)) {
                matchingRoutes.add(route);
            }
        }
        return matchingRoutes;
    }

    // Passenger management
    public void registerPassenger(Passenger passenger) {
        passengers.add(passenger);
    }

    public Passenger findPassenger(String id) {
        for (Passenger passenger : passengers) {
            if (passenger.getId().equals(id)) {
                return passenger;
            }
        }
        return null;
    }

    // Staff management
    public void addStaff(Staff member) {
        staff.add(member);
    }

    // Search operations
    public ArrayList<Train> searchAvailableTrains(TrainStation origin,
                                                    TrainStation destination) {
        ArrayList<Train> available = new ArrayList<>();
        for (Train train : trains) {
            if (train.getRoute() != null &&
                train.getRoute().getOrigin().equals(origin) &&
                train.getRoute().getDestination().equals(destination) &&
                train.getAvailableSeats() > 0) {
                available.add(train);
            }
        }
        return available;
    }

    // Getters
    public ArrayList<TrainStation> getStations() { return stations; }
    public ArrayList<Train> getTrains() { return trains; }
    public ArrayList<Route> getRoutes() { return routes; }
    public ArrayList<Passenger> getPassengers() { return passengers; }
    public ArrayList<Staff> getStaff() { return staff; }

    // Display statistics (override from BaseManagementSystem)
    @Override
    public void displaySystemStats() {
        System.out.println("\n═══════ SYSTEM STATISTICS ═══════");
        System.out.println("Total Stations: " + stations.size());
        System.out.println("Total Trains: " + trains.size());
        System.out.println("Total Routes: " + routes.size());
        System.out.println("Registered Passengers: " + passengers.size());
        System.out.println("Staff Members: " + staff.size());
        System.out.println("════════════════════════════════\n");
    }

    // Analytics - Revenue Analysis
    public void analyzeRevenue() {
        double totalRevenue = 0.0;
        for (Route route : routes) {
            totalRevenue += route.getBaseFare();  // Call dependency
        }
        System.out.println("Estimated Daily Revenue: $" + totalRevenue);
        logAction("Revenue analysis performed: $" + totalRevenue);
    }

    // Analytics - System Capacity
    public int getTotalCapacity() {
        int total = 0;
        for (Train train : trains) {
            total += train.getTotalSeats();  // Call dependency
        }
        return total;
    }
}

/**
 * ReportingSystem - Second God Class for testing multi-class file analysis
 * Handles all reporting and analytics - another anti-pattern!
 * This is a PERFECT MIRROR of the Python version
 */
class ReportingSystem {
    private TicketBookingSystem bookingSystem;
    private ArrayList<Object[]> reports;
    private java.util.HashMap<String, Object> metrics;
    private java.util.HashMap<String, Integer> cachedStats;

    public ReportingSystem(TicketBookingSystem bookingSystem) {
        this.bookingSystem = bookingSystem;
        this.reports = new ArrayList<>();
        this.metrics = new java.util.HashMap<>();
        this.cachedStats = null;
    }

    /**
     * Generate report of all stations
     */
    public ArrayList<java.util.HashMap<String, Object>> generateStationReport() {
        ArrayList<java.util.HashMap<String, Object>> report = new ArrayList<>();
        for (TrainStation station : bookingSystem.getStations()) {
            java.util.HashMap<String, Object> stationData = new java.util.HashMap<>();
            stationData.put("code", station.getStationCode());
            stationData.put("name", station.getName());
            stationData.put("trains", station.getAvailableTrains().size());
            report.add(stationData);
        }
        reports.add(new Object[]{"station", report});
        return report;
    }

    /**
     * Generate report of all trains
     */
    public ArrayList<java.util.HashMap<String, Object>> generateTrainReport() {
        ArrayList<java.util.HashMap<String, Object>> report = new ArrayList<>();
        for (Train train : bookingSystem.getTrains()) {
            java.util.HashMap<String, Object> trainData = new java.util.HashMap<>();
            trainData.put("id", train.getTrainNumber());
            trainData.put("name", train.getTrainName());
            trainData.put("seats", train.getTotalSeats());
            trainData.put("available", train.getAvailableSeats());
            report.add(trainData);
        }
        reports.add(new Object[]{"train", report});
        return report;
    }

    /**
     * Generate report of all passengers
     */
    public ArrayList<java.util.HashMap<String, Object>> generatePassengerReport() {
        ArrayList<java.util.HashMap<String, Object>> report = new ArrayList<>();
        for (Passenger passenger : bookingSystem.getPassengers()) {
            java.util.HashMap<String, Object> passengerData = new java.util.HashMap<>();
            passengerData.put("id", passenger.getPassengerId());
            passengerData.put("name", passenger.getName());
            passengerData.put("email", passenger.getEmail());
            report.add(passengerData);
        }
        reports.add(new Object[]{"passenger", report});
        return report;
    }

    /**
     * Calculate overall train occupancy
     */
    public double calculateOccupancyRate() {
        int totalSeats = 0;
        int bookedSeats = 0;
        for (Train train : bookingSystem.getTrains()) {
            totalSeats += train.getTotalSeats();
            bookedSeats += (train.getTotalSeats() - train.getAvailableSeats());
        }

        double rate = totalSeats > 0 ? (bookedSeats * 100.0 / totalSeats) : 0.0;
        metrics.put("occupancy_rate", rate);
        return rate;
    }

    /**
     * Calculate total revenue from all bookings
     */
    public double calculateRevenue() {
        double total = 0.0;
        for (Train train : bookingSystem.getTrains()) {
            if (train.getRoute() != null) {
                int booked = train.getTotalSeats() - train.getAvailableSeats();
                total += booked * train.getRoute().getBaseFare();
            }
        }

        metrics.put("total_revenue", total);
        return total;
    }

    /**
     * Get most popular routes by bookings
     */
    public ArrayList<java.util.Map.Entry<String, Integer>> getPopularRoutes(int limit) {
        java.util.HashMap<String, Integer> routeBookings = new java.util.HashMap<>();
        for (Train train : bookingSystem.getTrains()) {
            if (train.getRoute() != null) {
                String routeId = train.getRoute().getRouteId();
                int bookings = train.getTotalSeats() - train.getAvailableSeats();
                routeBookings.put(routeId, routeBookings.getOrDefault(routeId, 0) + bookings);
            }
        }

        ArrayList<java.util.Map.Entry<String, Integer>> sortedRoutes = new ArrayList<>(routeBookings.entrySet());
        java.util.Collections.sort(sortedRoutes, new java.util.Comparator<java.util.Map.Entry<String, Integer>>() {
            public int compare(java.util.Map.Entry<String, Integer> a, java.util.Map.Entry<String, Integer> b) {
                return b.getValue().compareTo(a.getValue());
            }
        });

        return new ArrayList<>(sortedRoutes.subList(0, Math.min(limit, sortedRoutes.size())));
    }

    /**
     * Calculate traffic through each station
     */
    public java.util.HashMap<String, Integer> getStationTraffic() {
        java.util.HashMap<String, Integer> traffic = new java.util.HashMap<>();
        for (Route route : bookingSystem.getRoutes()) {
            // Origin traffic
            String originCode = route.getOrigin().getStationCode();
            traffic.put(originCode, traffic.getOrDefault(originCode, 0) + 1);

            // Destination traffic
            String destCode = route.getDestination().getStationCode();
            traffic.put(destCode, traffic.getOrDefault(destCode, 0) + 1);
        }

        cachedStats = traffic;
        return traffic;
    }

    /**
     * Generate comprehensive system summary
     */
    public java.util.HashMap<String, Object> generateSummary() {
        java.util.HashMap<String, Object> summary = new java.util.HashMap<>();
        summary.put("total_stations", bookingSystem.getStations().size());
        summary.put("total_trains", bookingSystem.getTrains().size());
        summary.put("total_routes", bookingSystem.getRoutes().size());
        summary.put("total_passengers", bookingSystem.getPassengers().size());
        summary.put("occupancy_rate", calculateOccupancyRate());
        summary.put("total_revenue", calculateRevenue());
        summary.put("reports_generated", reports.size());

        metrics.putAll(summary);
        return summary;
    }

    /**
     * Clear cached statistics
     */
    public void clearCache() {
        cachedStats = null;
        metrics.clear();
    }

    /**
     * Export all generated reports
     */
    public java.util.HashMap<String, Object> exportAllReports() {
        java.util.HashMap<String, Object> export = new java.util.HashMap<>();
        export.put("reports", reports);
        export.put("metrics", metrics);
        export.put("summary", generateSummary());
        return export;
    }
}

/**
 * AdvancedReportingSystem - Subclass to test EXTEND dependencies in multi-class files.
 * Adds a small extra layer on top of ReportingSystem.
 */
class AdvancedReportingSystem extends ReportingSystem {
    public AdvancedReportingSystem(TicketBookingSystem bookingSystem) {
        super(bookingSystem);
    }

    public java.util.HashMap<String, Object> generateExecutiveSummary() {
        java.util.HashMap<String, Object> summary = generateSummary();
        summary.put("generated_by", "AdvancedReportingSystem");
        return summary;
    }
}

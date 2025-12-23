package TTS;

/**
 * Utility functions for Train Ticket System - no classes with state, only static methods.
 * This file provides helper functions used by other modules.
 */
public class TicketUtils {

    private static final double DEFAULT_PRICE_PER_KM = 0.15;
    private static final int DEFAULT_MAX_SEATS = 100;

    /**
     * Validate that a ticket price is valid (positive number).
     */
    public static boolean validateTicketPrice(double price) {
        return price > 0;
    }

    /**
     * Calculate total fare based on base fare and distance.
     */
    public static double calculateTotalFare(double baseFare, double distanceKm) {
        return calculateTotalFare(baseFare, distanceKm, DEFAULT_PRICE_PER_KM);
    }

    /**
     * Calculate total fare with custom price per km.
     */
    public static double calculateTotalFare(double baseFare, double distanceKm, double pricePerKm) {
        if (!validateTicketPrice(baseFare)) {
            throw new IllegalArgumentException("Base fare must be a positive number");
        }
        if (distanceKm < 0) {
            throw new IllegalArgumentException("Distance cannot be negative");
        }
        return baseFare + (distanceKm * pricePerKm);
    }

    /**
     * Format an amount as currency string.
     */
    public static String formatCurrency(double amount) {
        return formatCurrency(amount, "$");
    }

    /**
     * Format an amount as currency string with custom symbol.
     */
    public static String formatCurrency(double amount, String currencySymbol) {
        return String.format("%s%.2f", currencySymbol, amount);
    }

    /**
     * Calculate discounted price.
     */
    public static double calculateDiscount(double originalPrice, double discountPercent) {
        if (!validateTicketPrice(originalPrice)) {
            throw new IllegalArgumentException("Original price must be positive");
        }
        if (discountPercent < 0 || discountPercent > 100) {
            throw new IllegalArgumentException("Discount percent must be between 0 and 100");
        }
        double discountAmount = originalPrice * (discountPercent / 100.0);
        return originalPrice - discountAmount;
    }

    /**
     * Validate seat number is within valid range.
     */
    public static boolean validateSeatNumber(int seatNumber) {
        return validateSeatNumber(seatNumber, DEFAULT_MAX_SEATS);
    }

    /**
     * Validate seat number with custom max seats.
     */
    public static boolean validateSeatNumber(int seatNumber, int maxSeats) {
        return seatNumber >= 1 && seatNumber <= maxSeats;
    }

    /**
     * Generate a unique ticket ID.
     */
    public static String generateTicketId(String prefix, int counter) {
        return String.format("%s-%06d", prefix, counter);
    }

    /**
     * Generate ticket ID with default prefix.
     */
    public static String generateTicketId(int counter) {
        return generateTicketId("TKT", counter);
    }
}

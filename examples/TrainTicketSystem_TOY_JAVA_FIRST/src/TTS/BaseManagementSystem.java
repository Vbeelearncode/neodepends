package TTS;

/**
 * Base Management System - Abstract base class
 * All management systems extend this to provide logging and statistics
 */
public abstract class BaseManagementSystem {

    /**
     * Log an action for auditing purposes
     * @param action The action to log
     */
    protected void logAction(String action) {
        System.out.println("[LOG] " + action);
    }

    /**
     * Display system statistics
     * Must be implemented by subclasses
     */
    public abstract void displaySystemStats();
}

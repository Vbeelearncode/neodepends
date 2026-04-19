export interface Shape {
    area(): number;
    perimeter(): number;
}

export abstract class BaseShape implements Shape {
    public readonly label: string;

    constructor(label: string) {
        this.label = label;
    }

    abstract area(): number;
    abstract perimeter(): number;

    describe(): string {
        return `${this.label}: area=${this.area()}, perimeter=${this.perimeter()}`;
    }
}

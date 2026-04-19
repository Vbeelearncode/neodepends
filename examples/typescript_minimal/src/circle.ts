import { BaseShape } from "./shape";

export class Circle extends BaseShape {
    public radius: number;
    private precision: number = 4;

    constructor(radius: number) {
        super("circle");
        this.radius = radius;
    }

    area(): number {
        return Math.PI * this.radius * this.radius;
    }

    perimeter(): number {
        return 2 * Math.PI * this.radius;
    }

    format(value: number): string {
        return value.toFixed(this.precision);
    }
}

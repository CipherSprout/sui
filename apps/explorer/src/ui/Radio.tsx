// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as RadioGroupPrimitive from '@radix-ui/react-radio-group';
import { type ComponentPropsWithoutRef, type ElementRef, forwardRef } from 'react';

const RadioGroup = RadioGroupPrimitive.Root;

export type RadioOptionProps = {
	label: string;
	value: string;
	disabled?: boolean;
};

const RadioOption = forwardRef<
	ElementRef<typeof RadioGroupPrimitive.Item>,
	ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Item> & { label: string }
>(({ label, ...props }, ref) => (
	<RadioGroupPrimitive.Item
		ref={ref}
		className="flex flex-col rounded-md border border-transparent bg-white px-2 py-1 text-captionSmall font-semibold text-steel-dark hover:text-steel-darker disabled:cursor-default  disabled:text-gray-60  data-[state=checked]:border-steel data-[state=checked]:text-hero-dark"
		{...props}
	>
		{label}
	</RadioGroupPrimitive.Item>
));

export { RadioGroup, RadioOption };
